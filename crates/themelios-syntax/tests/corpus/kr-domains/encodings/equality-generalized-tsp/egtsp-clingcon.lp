% Equality generalized TSP (clingcon variant).
%
% Same problem as the baseline egtsp.lp: the salesman departs the start, visits
% exactly one vertex per subset, returns to the start, and minimizes the total
% tour cost.
%
% The clingo/clingcon split: a native-CP tour ordering. The subset-representative
% choice (visited/1) and the per-arc tour-edge choices (included/3) stay in ASP;
% the ordering moves to clingcon. Each vertex carries a position(V) CSP integer in
% [0, K], where K is the number of subsets: the start sits at 0, the K subset
% representatives occupy 1..K, and non-visited vertices keep a free value. A single
% &sum equation makes the destination's position one greater than the source's on
% every tour arc except the closing arc back to the start (which carries no
% equation, so the cycle can wrap from K back to 0).
%
% No explicit subtour-elimination clause is needed: a sub-cycle disjoint from the
% start would force either two equal positions among visited vertices (violating
% distinctness) or contradictory position equations, so the position layer alone
% admits only a single start-rooted cycle.
%
% On the all-different: clingcon 5.2.1 accepts &distinct only when its conditional
% literals come from static facts, not choice-derived atoms like visited/1. The
% pairwise integrity constraint below is the workaround, one per unordered pair of
% visited vertices; at practical E-GTSP sizes its O(|visited|^2) count is
% negligible. Cost optimization stays in ASP (a single #minimize over included/3,
% no lex priorities).

% Inputs (same as the baseline)

#defined vertex/1.
#defined edge/3.
#defined start/1.
#defined in_subset/2.

% Domain encoding (ASP, unchanged from the baseline)

% subset(S): S is a subset the salesman must take exactly one vertex from.
subset(S) :- in_subset(_,S).

% visited(V): V is on the tour. Exactly one vertex per subset, plus the start.
1 { visited(V) : in_subset(V,S) } 1 :- subset(S).

visited(V) :- start(V).

% The start must not be a member of any subset.
:- start(V), in_subset(V,_).

% included(Src,Dst,C): the edge from Src to Dst (cost C) is on the tour. One
% outgoing and one incoming tour edge per visited vertex.
1 { included(Src,Dst,C) : edge(Src,Dst,C), visited(Dst) } 1 :- visited(Src).
1 { included(Src,Dst,C) : edge(Src,Dst,C), visited(Src) } 1 :- visited(Dst).

% Tour ordering (clingcon)

% num_subsets(K): the subset count, the upper bound of the position domain. The
% K+1 stops (start plus one representative per subset) occupy positions 0..K.
num_subsets(K) :- K = #count{ S : subset(S) }.

% position(V): a CSP integer in [0, K] for every vertex. Non-visited vertices keep
% a free value; the pairwise all-different below restricts only visited ones.
&dom { 0..K } = position(V) :- vertex(V), num_subsets(K).

% The start sits at position 0.
&sum { position(V) } = 0 :- start(V).

% Pairwise all-different on visited positions (see the preamble for why this is
% pairwise rather than &distinct).
:- visited(V1), visited(V2), V1 < V2,
   &sum { position(V1) - position(V2) } = 0.

% Position consecutivity: on every tour arc whose destination is not the start,
% the destination's position is one greater than the source's. The closing arc to
% the start carries no equation, letting the cycle wrap back to 0.
&sum { position(Dst) - position(Src) } = 1 :-
    included(Src,Dst,_),
    not start(Dst).

% Optimization (ASP)

#minimize { C,Src,Dst : included(Src,Dst,C) }.

#show visited/1.
#show included/3.
#show start/1.
