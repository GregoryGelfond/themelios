% Task allocation with scheduling (clingcon variant).
%
% Same problem as the baseline variant-04.lp: assign one compatible agent per
% task and schedule the tasks under group, window, precedence, and per-agent
% serialization constraints, minimizing total assignment cost.
%
% The clingo/clingcon split: the Boolean/Datalog layer (assignment, grouping,
% compatibility, and the disjunctive ordering choice for serialization) stays in
% ASP; the timing layer moves into clingcon. Start times, end times, and
% durations are integer CSP variables, and the window, precedence, and
% serialization conditions become linear &sum constraints, propagated by the CP
% backend rather than grounded over a discrete time domain. clingcon reports the
% CSP values (start_time/end_time/dur per task) in its assignment output.

% Assumed inputs (supplied by a separate instance file):
%   task(T)                 "T is a task."
%   agent(A)                "A is an agent."
%   cost_of(A,T,C)          "C is the cost of assigning agent A to task T."
%   in_group(T,G)           "task T belongs to group G."
%   completion_time(A,T,D)  "D is how long agent A takes to complete task T."
%   time_window(T,E,L)      "task T must start no earlier than E and end no later than L."
%   before(X,Y)             "task X must complete before task Y starts."
%
% A task with no in_group/2 fact is ungrouped and may be assigned independently.

#defined task/1.
#defined agent/1.
#defined cost_of/3.
#defined in_group/2.
#defined completion_time/3.
#defined time_window/3.
#defined before/2.

% Domain encoding

% assigned_to(A,T): agent A is assigned to task T.
% Each task gets exactly one compatible agent.
1{ assigned_to(A,T) : agent(A), compatible_with(A,T) }1 :- task(T).

% Tasks sharing a group go to the same agent.
:- same_group(T1,T2), assigned_to(A1,T1), assigned_to(A2,T2), A1 != A2.

% CSP timing variables, one of each per task, on the domain [0, M] where M is the
% latest deadline in any window.
&dom { 0..M } = start_time(T) :- task(T), horizon(M).
&dom { 0..M } = end_time(T)   :- task(T), horizon(M).
&dom { 0..M } = dur(T)        :- task(T), horizon(M).

% A task's duration is its assigned agent's completion time.
&sum { dur(T) } = D :- assigned_to(A,T), completion_time(A,T,D).

% End = start + duration, entirely between CSP variables.
&sum { end_time(T) - start_time(T) - dur(T) } = 0 :- task(T).

% Each task starts no earlier than its window opens, and ends no later than it closes.
&sum { start_time(T) } >= Earliest :- time_window(T,Earliest,_).
&sum { end_time(T) } <= Latest :- time_window(T,_,Latest).

% Precedence, as one linear constraint between the two CSP variables.
&sum { end_time(T1) - start_time(T2) } <= 0 :- before(T1,T2).

% Agent serialization: same-agent task pairs may not overlap. A Boolean ASP
% choice picks the order for each unordered same-agent pair, and the chosen
% direction becomes a linear constraint on the CSP variables.
{ first(T1,T2) } :- same_agent(T1,T2), T1 < T2.

&sum { end_time(T1) - start_time(T2) } <= 0 :-
    first(T1,T2).

&sum { end_time(T2) - start_time(T1) } <= 0 :-
    same_agent(T1,T2),
    T1 < T2,
    not first(T1,T2).

% Solver directives

% Minimize the total assignment cost.
#minimize{ C,A,T : assigned_to(A,T), cost_of(A,T,C) }.

#show assigned_to/2.
% assigned_cost(A,T,C): the cost actually incurred — the cost table's entry for an
% assignment that was in fact made. This is the relation the objective above sums
% over; naming it makes it observable without also exposing the whole input table.
assigned_cost(A,T,C) :- assigned_to(A,T), cost_of(A,T,C).

#show assigned_cost/3.

% Auxiliary atoms

% compatible_with(A,T): agent A can take task T (a cost is defined for the pair).
compatible_with(A,T) :- cost_of(A,T,_).

% same_agent(T1,T2): T1 and T2 are assigned to the same agent.
% same_group(T1,T2): T1 and T2 share at least one group.
same_agent(T1,T2) :- assigned_to(A,T1), assigned_to(A,T2).
same_group(T1,T2) :- in_group(T1,G), in_group(T2,G).

% horizon(M): the latest deadline in any window; the upper bound of the CSP
% timing domains.
horizon(M) :- M = #max{ L : time_window(_,_,L) }.
