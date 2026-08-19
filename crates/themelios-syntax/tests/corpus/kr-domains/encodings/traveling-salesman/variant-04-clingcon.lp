% Time-windowed multi-depot traveling salesman (clingcon variant).
%
% Same problem as the baseline variant-04.lp: each salesman starts and ends at
% its own depot, every vertex is visited by exactly one salesman, each arrival
% respects its vertex's optional time window, and the total traversed cost is
% minimized.
%
% The clingo/clingcon split: the whole Boolean/Datalog layer (the traversed_by/4
% choice, visited_by/2 reachability, the every-vertex-visited, one-salesman, and
% return-to-depot constraints) stays in ASP; the timing layer moves into
% clingcon. visit_time(V,S) and tour_time(S) are integer CSP variables over
% [0, M]; per-arc time arithmetic is a set of &sum equations guarded on
% traversed_by/4, so they fire only on traversed arcs; and a link equation sets
% visit_time(depot,S) = tour_time(S), mirroring the baseline (where the depot's
% visit_time is the closing-arc value tour_time reads off). That link lets
% visit_time(V,S) be referenced uniformly for every V, so one time-window rule
% covers depots and non-depots alike.
%
% This is what relocating the arithmetic buys: in pure ASP the recursive
% visit_time(Dst,S,T1+ET) head expands the closure of partial-tour-duration sums
% at grounding time, which blows up even with the baseline's horizon filter. As
% CSP variables, the times are propagated during search, never materialized at
% grounding. Cost optimization stays in ASP (a single #minimize over
% traversed_by/4, no lex priorities, composes directly).

% Inputs (same as the baseline)

#defined vertex/1.
#defined edge/3.
#defined edge_time/3.
#defined salesman/1.
#defined depot/2.
#defined time_window/3.

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

% Optimization (ASP)

#minimize{ C,Src,Dst,S : traversed_by(Src,Dst,C,S) }.

#show traversed_by/4.
#show depot/2.
