% Scenario 01: Basic shortest path, the budget loose enough not to bind (clingcon).
% @expect sat
% @cost { 2 2 }
% @count optimal 1
% @optimal { start(a), included(a,b,1,0), included(b,c,1,0), end(c) }
% @elenctic solver clingcon
%
% The basic triangle from variant-01, lifted to the /4 edge signature (weight,
% resource cost). All resource costs are 0 and the cap is loose, so the
% minimum-weight path a -> b -> c wins, as in variant-01. The clingcon
% implementation propagates the resource-cost cap as a &sum CP constraint over
% the same included/4 observable.

#include "../../../encodings/shortest-path/variant-03-clingcon.lp".

vertex(a; b; c).

edge(
    a,b,1,0;
    b,c,1,0;
    a,c,3,0
).

start(a).
end(c).

maximum_cost(100).
