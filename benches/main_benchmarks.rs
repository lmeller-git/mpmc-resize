#![allow(clippy::std_instead_of_alloc, clippy::std_instead_of_core)]

use std::{
    hint::{black_box, spin_loop},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use crossbeam_queue::{ArrayQueue, SegQueue};
use mpmc_resize::{BoundedCollection, Resizable};

#[derive(Debug)]
pub struct ArrayQueueWrapper<T>(ArrayQueue<T>);

impl<T> BoundedCollection for ArrayQueueWrapper<T> {
    type Item = T;

    fn with_capacity(capacity: usize) -> Self {
        Self(ArrayQueue::new(capacity))
    }

    fn try_push(&self, item: Self::Item) -> Result<(), Self::Item> {
        self.0.push(item)
    }

    fn try_pop(&self) -> Option<Self::Item> {
        self.0.pop()
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

fn bench_steady_state_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("steady_state_overhead");
    const ITEMS: usize = 50_000;
    const CAPACITY: usize = 65_536;

    group.throughput(Throughput::Elements(ITEMS as u64));

    group.bench_function("raw_array_queue", |b| {
        b.iter(|| {
            let q = ArrayQueueWrapper::<usize>::with_capacity(CAPACITY);
            thread::scope(|s| {
                s.spawn(|| {
                    for i in 0..ITEMS {
                        while q.try_push(i).is_err() {
                            spin_loop();
                        }
                    }
                });
                s.spawn(|| {
                    let mut popped = 0;
                    while popped < ITEMS {
                        if q.try_pop().is_some() {
                            popped += 1;
                        } else {
                            spin_loop();
                        }
                    }
                });
            });
        });
    });

    group.bench_function("resizable_array_queue", |b| {
        b.iter(|| {
            let q = Resizable::<ArrayQueueWrapper<usize>>::with_capacity(CAPACITY);
            thread::scope(|s| {
                s.spawn(|| {
                    for i in 0..ITEMS {
                        while q.try_push(i).is_err() {
                            spin_loop();
                        }
                    }
                });
                s.spawn(|| {
                    let mut popped = 0;
                    while popped < ITEMS {
                        if q.try_pop().is_some() {
                            popped += 1;
                        } else {
                            spin_loop();
                        }
                    }
                });
            });
        });
    });

    group.finish();
}

fn bench_resize_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("active_resizing");
    const ITEMS: usize = 100_000;
    const INITIAL_CAPACITY: usize = 512;
    const MAX_CAPACITY: usize = 65_536 / 2;

    group.throughput(Throughput::Elements(ITEMS as u64));

    group.bench_function("arrayqueue_no_resize", |b| {
        b.iter(|| {
            let q = ArrayQueueWrapper::with_capacity(INITIAL_CAPACITY);
            let done = AtomicBool::new(false);
            let done_ref = &done;

            thread::scope(|s| {
                s.spawn(|| {
                    while !done_ref.load(Ordering::Relaxed) {
                        if q.is_full() {
                            black_box(42);
                        }
                        thread::yield_now();
                    }
                });

                s.spawn(|| {
                    for i in 0..ITEMS {
                        while q.try_push(i).is_err() {
                            spin_loop();
                        }
                    }
                });

                s.spawn(|| {
                    let mut popped = 0;
                    while popped < ITEMS {
                        if q.try_pop().is_some() {
                            popped += 1;
                        } else {
                            spin_loop();
                        }
                    }
                    done_ref.store(true, Ordering::Relaxed);
                });
            });
        });
    });

    group.bench_function("resizable_with_active_resizes", |b| {
        b.iter(|| {
            let q = Resizable::<ArrayQueueWrapper<usize>>::with_capacity(INITIAL_CAPACITY);
            let done = AtomicBool::new(false);
            let done_ref = &done;

            thread::scope(|s| {
                s.spawn(|| {
                    while !done_ref.load(Ordering::Relaxed) {
                        let current_cap = q.capacity();

                        if current_cap >= MAX_CAPACITY {
                            break;
                        }

                        if q.is_full() {
                            let next_cap = (current_cap * 2).min(MAX_CAPACITY);

                            if next_cap > current_cap {
                                let _ = q.resize(next_cap);
                            }
                        }
                        thread::yield_now();
                    }
                });

                s.spawn(|| {
                    for i in 0..ITEMS {
                        while q.try_push(i).is_err() {
                            spin_loop();
                        }
                    }
                });

                s.spawn(|| {
                    let mut popped = 0;
                    while popped < ITEMS {
                        if q.try_pop().is_some() {
                            popped += 1;
                        } else {
                            spin_loop();
                        }
                    }
                    done_ref.store(true, Ordering::Relaxed);
                });
            });
        });
    });

    group.finish();
}

fn bench_reordering_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("reordering_analysis");
    let mut total_duration = Duration::ZERO;
    let mut total_popped_items = 0usize;
    let mut total_out_of_order_events = 0usize;
    let mut max_observed_k = 0usize;

    group.bench_function("empirical_k_fifo_measurement", |b| {
        b.iter_custom(|iters| {
            for _ in 0..iters {
                const ITEMS: usize = 20_000;
                let q = Resizable::<ArrayQueueWrapper<usize>>::with_capacity(64);
                let done = AtomicBool::new(false);
                let popped_sequence = SegQueue::new();

                let start = std::time::Instant::now();

                thread::scope(|s| {
                    s.spawn(|| {
                        let mut cap = 64;
                        while !done.load(Ordering::Relaxed) {
                            cap += 16;
                            let _ = q.resize(cap);
                            thread::yield_now();
                        }
                    });

                    s.spawn(|| {
                        for i in 0..ITEMS {
                            while q.try_push(i).is_err() {
                                spin_loop();
                            }
                        }
                    });

                    s.spawn(|| {
                        let mut count = 0;
                        while count < ITEMS {
                            if let Some(item) = q.try_pop() {
                                popped_sequence.push(item);
                                count += 1;
                            } else {
                                spin_loop();
                            }
                        }
                        done.store(true, Ordering::Relaxed);
                    });
                });

                total_duration += start.elapsed();

                let mut received = Vec::with_capacity(ITEMS);
                while let Some(item) = popped_sequence.pop() {
                    received.push(item);
                }

                let mut highest_seen = 0;
                for val in received {
                    total_popped_items += 1;
                    if val < highest_seen {
                        total_out_of_order_events += 1;
                        let displacement = highest_seen - val;
                        if displacement > max_observed_k {
                            max_observed_k = displacement;
                        }
                    } else {
                        highest_seen = val;
                    }
                }
            }
            total_duration
        });
    });

    group.finish();

    println!("\n--- Empirical k-FIFO Reordering Results ---");
    println!("Total Elements Processed : {}", total_popped_items);
    println!("Reordered Element Events : {}", total_out_of_order_events);
    println!(
        "Reorder Rate             : {:.4}%",
        (total_out_of_order_events as f64 / total_popped_items as f64) * 100.0
    );
    println!("Max Displacement Rank (k): {}", max_observed_k);
    println!("-------------------------------------------\n");
}

criterion_group!(
    benches,
    bench_steady_state_overhead,
    bench_resize_throughput,
    bench_reordering_metrics
);
criterion_main!(benches);
