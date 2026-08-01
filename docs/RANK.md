## Informal proof of the rank during a resize

Suppose we have N threads, K of which executing a `try_push` operation, M of which executing a `try_pop` operation and 1 executing a `resize` operation.
We denote the two internal buffers of the algorithm as `queue0` and `queue1`, where `queue0` is the currently actice queue before the call to `resize` and `queue1` is the currently active queue after the call to `resize`.

A reordering between two items can occur if two or more items that were pushed by non-overlapping operations are not ordered according to the ordering of operations.

For an item to be reorderd during a resize thus following condition must hold:
 - there must exist a schedule that allows two non-overlapping pushes to be reorderd

In the algorihtm `try_push` there exists a schedule that fulfills the first conditon:
 If one or mote items are `in-flight` while `try_pop` checks queue0 and the push gets finalized after `try_pop` has finished checking queue0, but before it has checked the newly allocated queue1,
 then there exists the possibility for one of the K threads executing one or more push operations, which get routed to queue1. After these operations have finished the popping thread will check queue1, find an item in it and return it.
 The first item pushed to queue0 and the first item pushed to queue1 have now been reordered.

From this we can deduce the rank (i.e. the upper bound of the window-size for a reordering event):

From condition 1 we know that for any reordering event at least two distinct threads executing push and pop are necessary and that the first item pushed to queue0 will be reordered with the first item pushed to queue1.
Thus if we K threads executing push, M threads executing pop and L threads executing push strictly after L of the K threads have returned,
then each of the K items pushed to queue0 can be reordered with one of the L items pushed to queue1 across all M popping threads.

From this it directly follows that
a) at most M items will be reordered AND any item K will be reordered by at most M + K slots, since the first of the K items can at most be reordered wiht the last of the M popped items.
b) the number of reordering events is further bounded by the number of items L availble to be reordered, since for a reordering to happen, at least two items are necessary in different queues.
   This bounds again both the number of reordered items to L and the rank to K + L using the same reasoning as before.

The upper bound of the rank can be further reduced by applying the strict FIFO ordering of the inner queues queue0 and queue1 to thre reasoning for conditon1:
Since the first item in queue0 will be reordered with the first item in queue1, and both queus are strictly FIFO, the kth item in queue0 will be reordered with the kth item in queue1.
Thus the rank is exactly K.

Even further ALL reorderings will be of exactly rank K and in essence a batch of up to K items will be reordered.

Now define a subset P of M not part of the schedule leading to condition1. These threads will not produce reordered items and instead work to reduce the total reordering.
If P threads succeed to pop a correctly ordered item before item P has been reordered, then this item will not be reorderd. Further these P items are now longer part of the reordered batch if items, thus reducing the effective rank of all subsequent reorderings of this batch to K - P.

Thus the rank of the queue during a resize event is

k <= K - P
n_items_reorderd <= min(K, L, M) - P
