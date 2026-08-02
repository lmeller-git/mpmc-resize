## Informal Proof of the Rank During a Resize

Suppose we have $N$ threads: $K$ of which are executing a `try_push` operation, $M$ of which are executing a `try_pop` operation, and 1 executing a `resize` operation.
We denote the two internal buffers of the algorithm as `queue0` and `queue1`, where `queue0` is the currently active queue before the call to `resize` and `queue1` is the currently active queue after the call to `resize`.

A reordering between two items can occur if two or more items that were pushed by non-overlapping operations are not ordered according to the ordering of operations.

For an item to be reordered during a resize, the following condition must hold:
* [Condition 1] There must exist a schedule that allows two non-overlapping pushes to be reordered.

In the algorithm `try_push`, there exists a schedule that fulfills this condition:
If one or more items are in-flight while `try_pop` checks `queue0` and the one ore more of the pushes get finalized after `try_pop` has finished checking `queue0`, but before it has checked the newly allocated `queue1`, then there exists the possibility for one or more of the $K$ threads executing one or more further push operations, which will get routed to `queue1`.
After these operations have finished, the popping thread will check `queue1`, find an item in it, and return it.
The first item pushed to `queue0` and the first item pushed to `queue1` have now been reordered.

In fact this is the only such schedule, because there exists exactly this window in push and pop.

From this, we can deduce the rank (i.e., the upper bound of the window size for a reordering event) and the delay (i.e. the upper bound of the number of times any item can be skipped):

From Condition 1, we know that for any reordering event, at least two distinct threads executing push and pop are necessary and that the first item pushed to `queue0` will be reordered with the first item pushed to `queue1`.
Thus, if we have $K$ threads executing push, $M$ threads executing pop, and $L$ threads executing push strictly after $L$ of the $K$ threads have returned, then each of the $K$ items pushed to `queue0` can be reordered with one of the $L$ items pushed to `queue1` across all $M$ popping threads.

From this, it directly follows that:
* **a)** At most $M$ items will be reordered AND any item $K$ will be reordered by at most $M + K$ slots, since the first of the $K$ items can at most be reordered with the last of the $M$ popped items.
* **b)** The number of reordering events is further bounded by the number of items $L$ available to be reordered, since for a reordering to happen, at least two items are necessary in different queues. This bounds again both the number of reordered items to $L$ and the rank to $K + L$ using the same reasoning as before.

At this point we have:

$$k \le \min(L, M) + K$$

$$I \le \min(L, M)$$

where $I$ is the total number of tiems an item has been skipped during this resize event, i.e. the delay.


The upper bound of the rank can be further reduced by applying the strict FIFO ordering of the inner queues `queue0` and `queue1` to the reasoning for Condition 1:
Since the first item in `queue0` will be reordered with the first item in `queue1`, and both queues are strictly FIFO, the $k$-th item in `queue0` will be reordered with the $k$-th item in `queue1`.
Thus, the rank is exactly $K$.

Even further, ALL reorderings will be of exactly rank $K$, and in essence a batch of up to $K$ items will be reordered.

Now define a subset $P$ of $M$ not part of the schedule leading to Condition 1. These threads will not produce reordered items and instead work to reduce the total reordering.
If $P$ threads succeed in popping a correctly ordered item before item $P$ has been reordered, then this item will not be reordered. Further, these $P$ items are no longer part of the reordered batch of items, thus reducing the effective rank of all subsequent reorderings of this batch to $K - P$.

Thus, the rank and delay of the queue during a resize event are:

$$k \le K - P$$

$$I \le \min(L, M) - P$$

where $$P \le \min(L, M)$$

Since $L$ is bounded only by schedule depth, we can in general assume $L$ to be infinite and remove it from the delay bound:

$$I \le M - P$$

Further, since $P$ is 0 in the worst case, the true upper bound for rank and delay is:

$$k \le K$$

$$I \le M$$

We can further see that the bound on $k$ is **tight**, because a scenario where $K = 1, M = 1$ (and thus $P = 0$) can trivially reach the upper bound of $k = 1$.

Note, however, that not all items in a reordered batch will have a displacement of exactly $K - P$, since $P$ is a dynamic parameter that changes over the course of that execution.
