% Scenario 08: The cap leaves two equal-weight routes; the tie-breaker decides. (clingcon)
% @expect sat
% @cost { 3 2 }
% @count optimal 1
% @optimal { start(s), included(s,b,2,1), included(b,t,1,1), end(t) }
% @elenctic solver clingcon
%
% Edges (weight, cost): s -> t (1,10), s -> b (2,1), b -> t (1,1), s -> a (1,1),
% a -> b (1,1). The direct s -> t is the lightest route at weight 1, but its cost
% of 10 exceeds maximum_cost(5), which here is the clingcon &sum bound rather
% than an ASP integrity constraint. Two routes remain and both weigh 3:
% s -> b -> t at two hops (cost 2) and s -> a -> b -> t at three (cost 3). Both
% fit the budget, so the CP bound cannot separate them, and the edge-count
% tie-breaker, which stays in ASP, decides. Without it both routes are optimal.

#include "../../../encodings/shortest-path/variant-03-clingcon.lp".

vertex(s; a; b; t).

edge(
    s,t,1,10;
    s,b,2,1;
    b,t,1,1;
    s,a,1,1;
    a,b,1,1
).

start(s).
end(t).

maximum_cost(5).
