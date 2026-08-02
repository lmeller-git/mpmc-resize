# Rank and Delay of Resizable

In the following a simplified model of the algorithm `Resizable` is shown. Synchronization points are labelled.
Note that the `?` operator denotes "return the result of the operation if it was successfull else nop".

## Algorithm

```text

enqueue:

  loop
P0: epoch = load(push_epoch)
P1: inc(active_pushes[epoch])
P2: if load(push_epoch) == epoch then
P3:   res = enqueue(queues[epoch], item)
P4:   dec(active_pushes[epoch])
      return res
    end_if
P5: dec(active_pushes[epoch])
  end_loop


dequeue:

  for _ in 0..2 do
    loop
O0:   push_epoch = load(push_epoch)
O1:   pop_epoch = load(pop_epoch)
      if pop_epoch != push_epoch then
O2:     register_and_dequeue(queues[pop_epoch])?

O3:     if load(active_pushes[pop_epoch]) == 0 then
O4:       register_and_dequeue(queues[pop_epoch])?
O5:       cmpxchg(pop_epoch, pop_epoch + 1)
          continue
        end_if
      end_if
O6:   res = register_and_dequeue(queues[push_epoch])
O7:   if failure(res) && load(push_epoch) != push_epoch then
        continue
      end_if
      if success(res) || push_epoch == pop_epoch then
        return res
      end_if
      break
    end_loop
  end_for
  return None


resize:

R0: push_epoch = load(push_epoch)
R1: pop_epoch = load(pop_epoch)
R2: if !check_if_eligible() then // check_if_eligible only allows a new resize, once push_epoch == pop_epoch and all stale reads/writes have migrated
      return false
    end_if
R3: wait(active_pushes[push_epoch + 1] == 0 && registrations[push_epoch + 1] == 0)
R4: swap(queues[push_epoch + 1], new_queue)
R5: inc(push_epoch)
    cleanup
    return true
```

To show an upper bound of the rank error and delay, we will take the role of an adversarial scheduler to deduce the schedule leading to a reordering event and then derive the bounds.

## Preconditons

We assume that the subqueues queue0 and queue1 are linearizable in enqueue and dequeue and exhibit strict FIFO ordering.

We further assume that the total number of concurrent operations is bounded.

## Definitions

Let $P$ be the set of possible operations $\lbrace\text{dequeue}, \text{enqueue}, \text{resize}\rbrace$.

Let $OP$ be the set of concurrently executing operations $\lbrace op_0, \dots, op_m \rbrace$.

let $E_i(t)$ denote the cumulative enqueue count up to time $t$, and $D_i(t)$ denote the cumulative dequeue count up to time $t$ for sub-queue $i$ with ($i \in \lbrace 0, 1 \rbrace$).

Consider an item $x$ enqueued at time $t_e$ and dequeued at time $t_d$.

A **reordering event** occurs when an item $y$ that is enqueued after $x$ ($t_e(x) < t_e(y)$) is dequeued before $x$ ($t_d(y) < t_d(x)$).

The **rank error** of an item $x$ at its dequeue time $t_d$ is defined as the number of items enqueued before $t_e$ that remain in the queue at time $t_d$:

$$\text{rank\\_error}(x) = \max\Big(0, \; E_0(t_e) - D_0(t_d)\Big) + \max\Big(0, \; E_1(t_e) - D_1(t_d)\Big)$$

The **rank error of the queue** is defined as the maximum rank error over all dequeued items:

$$\text{rank\\_error}_q = \max_x \big( \text{rank\\_error}(x) \big)$$

The **delay** of an item $x$ is defined as the number of items enqueued after $t_e$ that are dequeued before $t_d$:

$$\text{delay}(x) = \max\Big(0, \; D_0(t_d) - E_0(t_e)\Big) + \max\Big(0, \; D_1(t_d) - E_1(t_e)\Big)$$

## Reordering Schedule

To obtain a reordering event we use three concurrent operations in $P$: dequeue, enqueue and resize:

Let $Q$ be the empty queue at time $t_0$ with subqueues $\text{queue0}$ and $\text{queue1}$.

Operation $op_0$ will now call $\text{enqueue}(x_0)$ on $Q$ and will be preempted immediately before synchronization point $P3$.

Now operation $op_1$ will call $\text{resize}$ and run it to completion, introducing $\text{queue1}$ and routing traffic to it.

Afterwards operation $op_2$ calls $\text{dequeue}()$ on $Q$ and runs until synchronization point $O6$ at which point it gets preempted.

At this point both $\text{queue0}$ and $\text{queue1}$ are empty and $op_0$ resumes and runs to completion, enqueueing one item $x_0$ to $\text{queue0}$ at time $t_1$.
Now $op_4$ runs a full $\text{enqueue}(x_1)$, enqueueing one item $x_1$ to $\text{queue1}$ at time $t_2$.

At this point $op_2$ resumes, finds the item $x_1$ in $\text{queue1}$ and dequeues it at time $t_3$.

Item $x_1$ has now been reordered with item $x_0$ with

$$\text{rank\\_error}(x_1) = 1$$

$$\text{delay}(x_0) = 1$$

Note that in the above $t_2$ can be equal to $t_1$ if > 1 concurrent enqueue is allowed. This is used in the following, collapsing the schedule to $t_0, t_1, t_2$.

This is the only such schedule, because dequeue must observe an $\text{enqueue}(x_0)$ operation that is stalled on $\text{pop\\_epoch}$ and it must find at least one item enqueued strictly after $t_e(x_0)$ in $\text{queue1}$.
Thus enqueue must be preempted at $P3$ and dequeue must be preempted between $O3$ and $O6$.

By generalizing this schedule to $N$ concurrent operations, we can find the upper bounds for delay and rank of the queue.

We have now $K$ operations concurrently executing enqueue, at least one operation resize and $P$ operations concurrently executing dequeue.

By scheduling all $K + P + 1$ threads according to the above schedule, we obtain a state of $Q$ at $t_1$ of $\text{queue0}$ containing all $K$ items, enqueued at time points $\lbrace t_{e0, q0}, \dots, t_{ek, q0} \rbrace$, where all $t_{e*, q0} < t_1$ and $\text{queue1}$ containing 0 items.
After this point $t_1$, an unbounded number of items can be enqueued to $\text{queue1}$ at time points $\lbrace t_{e0, q1}, \dots, t_{em, q1} \rbrace$, where all $t_{e*, q1} > t_1$.

Now all $P$ operations executing dequeue resume, finding $n$ items in $\text{queue1}$ and dequeueing all of them at time points $\lbrace t_{d0, q1}, \dots, t_{dp, q1} \rbrace$.

Here we map all $P$ executions of dequeue to linearize sequentially in the interval $[t_1, t_2]$, but it is not necessary to assume that only these operations linearize here.
Any other concurrent enqueue or dequeue operations linearizing in this interval would either operate on the correctly ordered queue1 or correctly drain older items from queue0.
In either case, such interleavings can only ever reduce the rank and delay of this schedule. Thus we can ignore the influence of these operations and handle $P$ as though they were all linearized in one instant.
The same argument also applies to $K$.

Since we assume a bounded number of total concurrent operations $T$, $K$ and $P$ are bounded by $K + P + 1 \le T$.

### Tightness

The lower bounds of the upper bounds for rank and delay we derive from this schedule are tight.

Each resize event is separated completely, because a resize event will only succeed if all accesses to the old queue have migrated to the new queue. In this state the queue exhibits the same behaviour as the currently active subqueue.
Thus we can limit the schedule to a single resize event.

During a resize event the only time a reordering can happen is when items are scattered across both subqueues. For this to happen either stale items need to be in the old subqueue, or enqueue must observe an epoch switch immediately before $P3$.
Since stale items in the old subqueue are observed by all dequeues happening before, during and after the resize, these will not be reordered.
Thus it follows that the constructed schedule is indeed necessary, also providing the upper bound for rank and delay.

## Delay and Rank bounds

The upper bounds of rank and delay of the items are thus as follows:

### Rank

For any younger item $y_j$ enqueued into $\text{queue1}$ at $t_e(y_j) > t_1$ and dequeued by one of the $P$ dequeue operations at time $t_d(y_j)$:

* **$\text{queue0}$:** $E_0(t_e(y_j)) = K$ older items were enqueued before $t_1$, but none have been dequeued from $\text{queue0}$ yet ($D_0(t_d(y_j)) = 0$). Thus, $\max\big(0, \; E_0(t_e(y_j)) - D_0(t_d(y_j))\big) = K$.
* **$\text{queue1}$:** Items within $\text{queue1}$ are extracted in FIFO order, so $E_1(t_e(y_j)) - D_1(t_d(y_j)) = 0$.

Substituting into the rank error formula:

$$\text{rank\\_error}(y_j) = \max\Big(0, \; E_0(t_e(y_j)) - D_0(t_d(y_j))\Big) + \max\Big(0, \; E_1(t_e(y_j)) - D_1(t_d(y_j))\Big) = K + 0 = K$$

From this the rank error of the queue $q$ directly follows:

$$\text{rank\\_error}_q = \max_y \big( \text{rank\\_error}(y) \big) \le K$$

### Delay

For any older item $x_i$ in $\text{queue0}$ enqueued at $t_e(x_i) < t_1$ and dequeued at time $t_d(x_i)$ after $t_2$:

* **$\text{queue0}$:** Within $\text{queue0}$, due to FIFO ordering no younger items will be reordered before $x_i$, so $D_0(t_d(x_i)) - E_0(t_e(x_i)) = 0$.
* **$\text{queue1}$:** At time $t_e(x_i)$, no items had been enqueued into $\text{queue1}$ yet ($E_1(t_e(x_i)) = 0$). Before $x_i$ is extracted, all $P$ operations resume and dequeue $P$ younger items from $\text{queue1}$ ($D_1(t_d(x_i)) = P$).

Substituting into the delay formula:

$$\text{delay}(x_i) = \max\Big(0, \; D_0(t_d(x_i)) - E_0(t_e(x_i))\Big) + \max\Big(0, \; D_1(t_d(x_i)) - E_1(t_e(x_i))\Big) = 0 + P = P$$

From this the delay of the queue $q$ directly follows:

$$\text{delay}(x_i) \le P$$

# References

Definitions were taken from:

```text
  Kåre von Geijer, Philippas Tsigas, Elias Johansson, and Sebastian
  Hermansson. 2025. Balanced Allocations over Efficient Queues:
  A Fast Relaxed FIFO Queue . In The 30th ACM SIGPLAN Annual Sym-
  posium on Principles and Practice of Parallel Programming (PPoPP
  ’25), March 1–5, 2025, Las Vegas, NV, USA. ACM, New York, NY, USA,
  14 pages. https://doi.org/10.1145/3710848.3710892
```
