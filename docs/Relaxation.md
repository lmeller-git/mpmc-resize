In the following a simplified model of the algorithm `Resizable` is shown. Synchronization points are labelled.
Note that the `?` operator denotes "return the result of the operation, if it was successfull else nop".

```text

enqueue:

  loop
P0: epoch = load(push_epoch)
P1: inc(active_pushes[epoch])
P2: if load(push_epoch) == epoch then
P3:   res = push(queues[epoch], item)
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
O2:     register_and_pop(queues[pop_epoch])?

O3:     if load(active_pushes[pop_epoch]) == 0 then
O4:       register_and_pop(queues[pop_epoch])?
O5:       cmpxchg(pop_epoch, pop_epoch -> pop_epoch + 1)
          continue
        end_if
      end_if
O6:   res = register_and_pop(queues[push_epoch])
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
    if !check_if_eligible() then // check_if_eligible only allows a new resize, once push_epoch == pop_epoch and all stale reads/writes have migrated
      return false
    end_if
R2: wait(active_pushes[push_epoch + 1] == 0 && registrations[push_epoch + 1] == 0)
R3: swap(queues[push_epoch + 1], new_queue)
R4: inc(push_epoch)
    cleanup
    return true
```

To show an upper bound of the rank error and delay, we will take the role of an adversarial scheduler to deduce the schedule leading to a reordering event and then derive the bounds.

## Definitions

Let $P$ be the set of possible operations $\{\text{dequeue}, \text{enqueue}, \text{resize}\}$.

Let $OP$ be the set of concurrently executing operations $\{op_0, \dots, op_m\}$.

let $E_i(t)$ denote the cumulative enqueue count up to time $t$, and $D_i(t)$ denote the cumulative dequeue count up to time $t$ for sub-queue $i$ with ($i \in \{1, 2\}$).

Consider an item $x$ enqueued at time $t_e$ and dequeued at time $t_d$.

A **reordering event** occurs when an item $y$ that is enqueued after $x$ ($t_e(x) < t_e(y)$) is dequeued before $x$ ($t_d(y) < t_d(x)$).

The **rank error** of an item $x$ at its dequeue time $t_d$ is defined as the number of items enqueued before $t_e$ that remain in the queue at time $t_d$:

$$\text{rank\_error}(x) = \max\Big(0, \; E_1(t_e) - D_1(t_d)\Big) + \max\Big(0, \; E_2(t_e) - D_2(t_d)\Big)$$

The **rank error of the queue** is defined as the maximum rank error over all dequeued items:

$$\text{rank\_error}_q = \max_x \big( \text{rank\_error}(x) \big)$$

The **delay** of an item $x$ is defined as the number of items enqueued after $t_e$ that are dequeued before $t_d$:

$$\text{delay}(x) = \max\Big(0, \; D_1(t_d) - E_1(t_e)\Big) + \max\Big(0, \; D_2(t_d) - E_2(t_e)\Big)$$

By vector duality, both quantities are upper-bounded by the 2-dimensional $\ell_1$-norm distance between the enqueue vector at $t_e$ and the dequeue vector at $t_d$:

$$\text{rank\_error}(x), \; \text{delay}(x) \le \big| E_1(t_e) - D_1(t_d) \big| + \big| E_2(t_e) - D_2(t_d) \big| = \| E(t_e) - D(t_d) \|_1$$

## Reordering Schedule

To obtain a reordering event we use three concurrent operations in $P$: dequeue, enqueue and resize:

Let Q be the empty queue at time t_0 with subqueues queue0 and queue1.

Operation $op_0$ will now call $enqueue(x_0)$ on Q and will be preempted at before at synchronization point $P3$.

Now operation $op_1$ will call $resize$ and run it to completion, introducing queue1 and routing traffic to it.

Afterwards operation $op_2$ calls $dequeue()$ on Q and runs until synchronization point $O6$ at which point it gets preempted.

At this point both queue0 and queue1 are empty and $op_0$ resumes and runs to completion, enqueueing one item $x_0$ to queue0 at time $t_1$.
Now $op_4$ runs a full $enqueue(x_1)$, enqueueing one item $x_1$ to queue1 at time $t_2$.

At this point $op_2$ resumes, finds the item $x_1$ in queue1 and dequeues it at time $t_3$.

Item $x_1$ has now been reordered with item $x_0$ with

$$\text{rank\_error}(x_1) = 1$$

$$\text{delay}(x_0) = 1$$

Note that in the above $t_2$ can be equal to $t_1$ if > 1 concurrent enqueue is allowed. This is used in the following, collapsing the schedule to $t_0, t_1, t_2$.


This is the only such schedule, because dequeue must observe an enqueue($x_0$) operation that is stalled on $pop_epoch$ and it must find at least one item enqueued strictly after $t_e(x_0)$ in queue1.
Thus enqueue must be preempted at $P3$ and dequeue must be preempted between $O3$ and $O6$.

By generalizing this schedule to $N$ concurrent operations, we can find the upper bounds for delay and rank of the queue.

We have now $K$ operations concurrently executing enqueue, at least one operation resize and $P$ operations concurrently executing dequeue.

By scheduling all $K + M + 1$ threads according to the above schedule, we obtain a state of Q at $t_1$ of queue0 containing all K items, enqueued at time points $\{t_e0_q0, \dots, t_ek_q0\}$, where all $t_e*_q0 < t_1$ and queue1 containing 0 items.
After this point $t_1$, an unbounded number of items can be enqueued to queue1 at time points $\{t_e0_q1, \dots, t_em_q1\}$, where all $t_e*_q1 > t_1$.

Now all $P$ operations executing dequeue resume, finding $n$ items in queue1 and dequeueing all of them at time points $\{t_d0_q1, \dots, t_dp_q1\}$.

We can fold all $P$ executions of dequeue into a single time point $t_1 \l t_de \le t_2$ and all $K$ executions of enqueue into a single time point $t_0 \l t_en \le t_1$, because the reordering event itself is linearizable on $t_1$.
Any of the $K$ operations that enqueues after $t_1$ will be reorderd again by $P$ dequeue operations according to teh above schedule. Thus $K$ is strictly fixed and all $P$ can be folded into the initial $P$.

## Delay and Rank bounds

The rank and delay of the items are now bounded as follows:

### Rank

For any younger item $y_j$ enqueued into $\text{queue1}$ at $t_e(y_j) > t_1$ and dequeued by one of the $P$ dequeue operations at time $t_d(y_j)$:

* **$\text{queue0}$ count:** $E_0(t_e(y_j)) = K$ older items were enqueued before $t_1$, but none have been dequeued from $\text{queue0}$ yet ($D_0(t_d(y_j)) = 0$). Thus, $\max\big(0, \; E_0(t_e(y_j)) - D_0(t_d(y_j))\big) = K$.
* **$\text{queue1}$ count:** Items within $\text{queue1}$ are extracted in FIFO order, so $E_1(t_e(y_j)) - D_1(t_d(y_j)) = 0$.

Substituting into the rank error formula:

$$\text{rank\_error}(y_j) = \max\Big(0, \; E_0(t_e(y_j)) - D_0(t_d(y_j))\Big) + \max\Big(0, \; E_1(t_e(y_j)) - D_1(t_d(y_j))\Big) = K + 0 = K$$

From this the rank error of the queue q directly follows:

$$\text{rank\_error}_q = \max_y \big( \text{rank\_error}(y) \big) = K \le P_{\text{push}}$$

### Delay

For any older item $x_i$ in $\text{queue0}$ enqueued at $t_e(x_i) < t_1$ and dequeued at time $t_d(x_i)$ after $t_2$:

* **$\text{queue0}$ count:** Within $\text{queue0}$, due to FIFO ordering $x_i$, so $D_0(t_d(x_i)) - E_0(t_e(x_i)) = 0$.
* **$\text{queue1}$ count:** At time $t_e(x_i)$, no items had been enqueued into $\text{queue1}$ yet ($E_1(t_e(x_i)) = 0$). Before $x_i$ is extracted, all $P$ operations resume and dequeue $P$ younger items from $\text{queue1}$ ($D_1(t_d(x_i)) = P$).

Substituting into the delay formula:

$$\text{delay}(x_i) = \max\Big(0, \; D_0(t_d(x_i)) - E_0(t_e(x_i))\Big) + \max\Big(0, \; D_1(t_d(x_i)) - E_1(t_e(x_i))\Big) = 0 + P = P$$

From this the delay of the queue q directly follows:

$$\text{delay}(x_i) \le P \le P_{\text{pop}}$$

