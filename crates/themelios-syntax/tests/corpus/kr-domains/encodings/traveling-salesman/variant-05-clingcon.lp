% Persistent-surveillance multi-depot traveling salesman (clingcon variant).
%
% Same problem as the baseline variant-05.lp: every vertex is visited by exactly
% one salesman patrolling a fixed cycle from its depot, each arrival respects the
% vertex's optional time window, each tour duration respects the maximum revisit
% interval of every vertex on it, and the total traversed cost is minimized.
%
% The clingo/clingcon split is identical to variant-04-clingcon: the whole
% Boolean/Datalog layer stays in ASP, and the timing layer (visit_time(V,S),
% tour_time(S), per-arc arithmetic, time-window bounds) is expressed in clingcon.
% The surveillance requirement added in v05 is one more &sum bound on
% tour_time(S), guarded on visited_by/2 and max_revisit_interval/2. The
% visit_time(depot,S) = tour_time(S) link is preserved, so downstream constraints
% reference visit_time(V,S) uniformly; the surveillance rule reads tour_time(S)
% directly, since that is the patrol period. Cost optimization stays in ASP (a
% single #minimize over traversed_by/4, no lex priorities).

% Inputs (same as the baseline)

#defined vertex/1.
#defined edge/3.
#defined edge_time/3.
#defined salesman/1.
#defined depot/2.
#defined time_window/3.
#defined max_revisit_interval/2.

% Domain encoding (ASP, unchanged from the baseline)

% traversed_by(Src,Dst,C,S): the edge from Src to Dst (cost C) is traversed by
% salesman S. At most one outgoing and one incoming edge per (vertex, salesman).
0{ traversed_by(Src,Dst,C,S) : edge(Src,Dst,C) }1 :-
    salesman(S),
    vertex(Src).

0{ traversed_by(Src,Dst,C,S) : edge(Src,Dst,C) }1 :-
    salesman(S),
    vertex(Dst).

% visited_by(V,S): V is visited by S (reachable from S's depot).
visited_by(Dst,S) :-
    depot(S,Src),
    traversed_by(Src,Dst,_,S).

visited_by(Dst,S) :-
    vertex(Src),
    visited_by(Src,S),
    traversed_by(Src,Dst,_,S).

% visited(V): V is visited by some salesman.
visited(V) :- vertex(V), visited_by(V,_).

% No salesman may traverse an edge leaving a vertex it does not visit.
:- traversed_by(Src,_,_,S), not visited_by(Src,S).

% Every vertex must be visited.
:- vertex(V), not visited(V).

% No vertex is shared by two salesmen.
:- vertex(V), #count{ S : visited_by(V,S) } > 1.

% returns_to_depot(S): salesman S returns to its depot.
returns_to_depot(S) :- depot(S,V), visited_by(V,S).

% Every salesman must return to its depot.
:- salesman(S), not returns_to_depot(S).

% Timing layer (clingcon)

% horizon(M): an upper bound on any tour time (the sum of all edge_time/3, since
% each edge is used at most once per salesman); the CSP domain's upper bound.
horizon(M) :- M = #sum{ ET,Src,Dst : edge_time(Src,Dst,ET) }.

% visit_time(V,S) per (vertex, salesman), depots included, and tour_time(S) per
% salesman, as CSP integers over [0, M].
&dom { 0..M } = visit_time(V,S) :- vertex(V), salesman(S), horizon(M).
&dom { 0..M } = tour_time(S)    :- salesman(S), horizon(M).

% Link: a depot's visit_time equals its salesman's tour_time, so downstream
% constraints can reference visit_time(V,S) uniformly for any V.
&sum { tour_time(S) - visit_time(D,S) } = 0 :- depot(S,D).

% Depot-outgoing hop: the first arrival sits at the depot arc's edge_time. The
% not depot(S,Dst) guard excludes the self-loop, handled below.
&sum { visit_time(Dst,S) } = ET :-
    depot(S,Src),
    traversed_by(Src,Dst,_,S),
    edge_time(Src,Dst,ET),
    not depot(S,Dst).

% Intermediate hop: visit_time accumulates edge_time along a non-depot arc.
&sum { visit_time(Dst,S) - visit_time(Src,S) } = ET :-
    vertex(Src),
    traversed_by(Src,Dst,_,S),
    edge_time(Src,Dst,ET),
    not depot(S,Src), not depot(S,Dst).

% Return-to-depot hop: the closing arc carries the last visit_time up to
% tour_time(S), closing the cycle.
&sum { tour_time(S) - visit_time(Src,S) } = ET :-
    vertex(Src),
    depot(S,D),
    traversed_by(Src,D,_,S),
    edge_time(Src,D,ET),
    not depot(S,Src).

% Degenerate depot self-loop: the whole tour is one arc D -> D, so tour_time(S) is
% that arc's edge_time. (Rare in TSP instances, but kept for parity with the
% baseline.)
&sum { tour_time(S) } = ET :-
    depot(S,D),
    traversed_by(D,D,_,S),
    edge_time(D,D,ET).

% Time windows (clingcon)

% Bound visit_time(V,S) when V is on S's tour. The depot link above makes this one
% rule cover depot windows too (the bound propagates to tour_time(S)).
&sum { visit_time(V,S) } >= Min :-
    visited_by(V,S),
    time_window(V,Min,_).

&sum { visit_time(V,S) } <= Max :-
    visited_by(V,S),
    time_window(V,_,Max).

% Persistent-surveillance constraint (clingcon)

% A salesman patrols a fixed cycle of length tour_time(S), so the gap between
% consecutive visits to any V on its tour equals tour_time(S). Bound tour_time(S)
% above by the maximum revisit interval of every V it visits; a vertex with no
% max_revisit_interval/2 fact is unconstrained.
&sum { tour_time(S) } <= M :-
    visited_by(V,S),
    max_revisit_interval(V,M).

% Optimization (ASP)

#minimize{ C,Src,Dst,S : traversed_by(Src,Dst,C,S) }.

#show traversed_by/4.
#show depot/2.
