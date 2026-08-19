% Resource-constrained shortest path with ordering (clingcon variant).
%
% Same problem as the baseline variant-04.lp: minimize the total weight of a
% start-to-end path under a hard resource-cost cap and before/after ordering
% constraints, with edge count as a tie-breaker.
%
% The clingo/clingcon split mirrors variant-03-clingcon: the whole Boolean/
% Datalog layer (edge choice, the path-structure constraints, reachability, the
% reachable_from/2 transitive closure, end-reachability, the no-dangling-edge
% and ordering constraints, weight aggregation, the lexicographic minimize)
% stays in ASP; only the resource-cost cap moves into clingcon as a &sum
% constraint over an ASP/CSP-bridged 0/1 shadow x(Src,Dst,W,C), where the CP
% backend propagates the bound during search and prunes partial assignments
% that already exceed the budget.
% The minimize stays in ASP because clingcon's &minimize has no priority
% mechanism that composes with #minimize.

% Inputs (same as the baseline)

#defined vertex/1.
#defined edge/4.
#defined start/1.
#defined end/1.
#defined before/2.
#defined after/2.
#defined maximum_cost/1.

% Domain encoding

% included(X,Y,W,C): the edge from X to Y (weight W, resource cost C) lies on the
% candidate path.
{ included(Src,Dst,W,C) : edge(Src,Dst,W,C) }.

% Path structure. Together these four make the included edges a simple path: at
% most one included edge leaves a vertex and at most one enters it, nothing
% enters the start, and nothing leaves the end. The ordering constraints below
% depend on this: on a path, reachable_from/2 is the visitation order, whereas
% over an arbitrary edge set it is only reachability.
%
% Only the out-degree bound and the no-edge-out-of-the-end rule are independent.
% The other two follow from them together with the no-dangling-edge constraint
% below: capping out-degree makes the walk from the start deterministic,
% no-dangling puts every included edge on that walk, nothing-leaves-end forces
% the walk to terminate, and a terminating deterministic walk cannot revisit a
% vertex. They are stated anyway, so that the object is read off the program
% rather than derived.
:- vertex(V), #count{ Dst,W,C : included(V,Dst,W,C) } > 1.
:- vertex(V), #count{ Src,W,C : included(Src,V,W,C) } > 1.
:- included(_,V,_,_), start(V).
:- included(V,_,_,_), end(V).

% reachable_from(X,Y): Y is reachable from X along included edges (transitive).
reachable_from(Src,Dst) :-
    included(Src,Dst,_,_).

reachable_from(Src,Dst) :-
    vertex(Tmp),
    included(Src,Tmp,_,_),
    reachable_from(Tmp,Dst).

% reachable(V): V is reachable from the start.
reachable(Src) :-
    start(Src).

reachable(Dst) :-
    start(Src),
    reachable_from(Src,Dst).

% The end must be reachable from the start.
:- end(Src), not reachable(Src).

% No included edge may leave an unreachable vertex.
:- included(Src,_,_,_), not reachable(Src).

% Respect the ordering constraints.
:- before(X,Y), reachable(Y), not reachable_from(X,Y).
:- after(X,Y), reachable(Y), not reachable_from(Y,X).

% ASP/CSP bridge

% A 0/1 CSP shadow per edge.
&dom { 0..1 } = x(Src,Dst,W,C) :- edge(Src,Dst,W,C).

% Force x(...) to mirror the truth of included(...).
:- included(Src,Dst,W,C), &sum { x(Src,Dst,W,C) } = 0.

:- edge(Src,Dst,W,C),
   not included(Src,Dst,W,C),
   &sum { x(Src,Dst,W,C) } = 1.

% Hard linear constraint (clingcon)

% Resource-cost cap. The CP backend propagates this bound during search,
% pruning partial assignments that already overflow the budget.
&sum { C*x(Src,Dst,W,C) : edge(Src,Dst,W,C) } <= M :- maximum_cost(M).

% Optimization (ASP)

weight(W) :- W = #sum { Wt,Src,Dst,Ct : included(Src,Dst,Wt,Ct) }.

#minimize { W@2 : weight(W) }.
#minimize { 1@1,Src,Dst,W,C : included(Src,Dst,W,C) }.

#show included/4.
#show start/1.
#show end/1.
