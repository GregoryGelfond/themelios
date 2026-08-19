% Task scheduling with co-optimal schedules (clingcon variant).
% @expect sat
% @cost { 2 }
% @count optimal 2
% @cautious optimal { assigned_to(a1,t1), assigned_to(a1,t2) }
% @brave optimal { first(t1,t2) }
% @assign optimal { start_time(t1)=3, start_time(t2)=0 }
% @optimal { assigned_to(a1,t1), assigned_to(a1,t2), first(t1,t2) } where { start_time(t1)=0, start_time(t2)=3 }
% @note Opt(P) fixes the assignment and reshuffles only the schedule: two optimal models (t1 then t2, and t2 then t1), equal in cost and agent assignment, differing only in the CSP start times.
% @note The joint witness is load-bearing: @brave optimal { first(t1,t2) } and @assign optimal { start_time(t2)=0 } each hold, but on different optimal models, so @optimal { ..., first(t1,t2) } where { start_time(t2)=0 } would fail. The where-clause binds the shown order and the schedule on one model.
% @elenctic solver clingcon
%
% The corpus's worked example of the clingcon theory-witness surface: the optimal-base
% assignment binding (assign-optimal) and the joint witness that ties a shown projection
% to a theory assignment on one model (the where-clause on optimal/model). It is
% clingcon-only by design: those tags read the theory half of the observable (the CSP
% schedule), which a pure-ASP baseline does not expose on the same footing, so there is
% no paired clingo encoding here.
%
% The problem mirrors task-allocation/variant-04: assign one compatible agent per task
% to minimize cost, with start and end times as integer CSP variables and per-agent
% serialization as linear constraints. The instance is tuned so the optimal class is a
% co-optimal reshuffle (the assignment is forced, but the schedule is not), exactly the
% situation the theory witnesses were built to describe.
%
% Why two optimal models. Agent a1 is cheap for both tasks and a2 is expensive, so every
% optimal solution puts both tasks on a1 (cost 2; any use of a2 costs at least 6). Sharing
% one agent forces the two tasks to run in series, and the tight [0, 6] window pins each
% ordering to a single schedule: t1-then-t2 (t1 at 0, t2 at 3) or t2-then-t1 (t2 at 0, t1
% at 3). The serialization order is the choice atom first(t1,t2), shown so the two optimal
% models are distinguishable in the projection, not only in the theory assignment.

% The instance

task(t1; t2).

agent(a1; a2).

cost_of(
    a1,t1,1;
    a1,t2,1;
    a2,t1,5;
    a2,t2,5
).

completion_time(
    a1,t1,3;
    a1,t2,3;
    a2,t1,3;
    a2,t2,3
).

time_window(
    t1,0,6;
    t2,0,6
).

% Domain encoding

% assigned_to(A,T): agent A is assigned to task T (exactly one compatible agent per task).
1{ assigned_to(A,T) : agent(A), compatible_with(A,T) }1 :- task(T).

% Timing layer (clingcon)

% start_time(T), end_time(T): task T's start and end, as integer CSP variables over [0, M].
&dom { 0..M } = start_time(T) :- task(T), horizon(M).
&dom { 0..M } = end_time(T)   :- task(T), horizon(M).

% A task runs for its assigned agent's completion time: end = start + duration.
&sum { end_time(T) - start_time(T) } = D :- assigned_to(A,T), completion_time(A,T,D).

% Each task starts no earlier than its window opens and ends no later than it closes.
&sum { start_time(T) } >= Earliest :- time_window(T,Earliest,_).
&sum { end_time(T) } <= Latest :- time_window(T,_,Latest).

% Serialization (clingcon)

% Two tasks on one agent may not overlap. A Boolean choice picks the order of each
% same-agent pair, and the chosen direction becomes a linear constraint on the times.
{ first(T1,T2) } :- same_agent(T1,T2), T1 < T2.

&sum { end_time(T1) - start_time(T2) } <= 0 :-
    first(T1,T2).

&sum { end_time(T2) - start_time(T1) } <= 0 :-
    same_agent(T1,T2), T1 < T2, not first(T1,T2).

% Optimization

% Minimize total assignment cost.
#minimize{ C,A,T : assigned_to(A,T), cost_of(A,T,C) }.

% Output

% The shown projection is the assignment plus the serialization order; the schedule
% (start_time/end_time) lives in the theory assignment, read by @assign / where.
#show assigned_to/2.
#show first/2.

% Auxiliary atoms

% compatible_with(A,T): A can take T (a cost is defined for the pair).
compatible_with(A,T) :- cost_of(A,T,_).

% same_agent(T1,T2): T1 and T2 are assigned to the same agent.
same_agent(T1,T2) :- assigned_to(A,T1), assigned_to(A,T2).

% horizon(M): the latest deadline in any window; the CSP timing domains' upper bound.
horizon(M) :- M = #max{ L : time_window(_,_,L) }.
