% Scenario 08: Both layers bind, then the tie-breaker decides. (clingcon)
% @expect sat
% @cost { 4 2 }
% @count optimal 1
% @optimal { start(s), included(s,b,3,1), included(b,t,1,1), end(t) }
% @elenctic solver clingcon
%
% Each layer rules out a different route and the tie-breaker settles what is
% left. Edges (weight, cost):
%   s -> c -> t        weight 2, cost 2    lightest, but skips b
%   s -> d -> b -> t   weight 3, cost 11   visits b, but overruns the cap
%   s -> b -> t        weight 4, cost 2    two hops
%   s -> a -> b -> t   weight 4, cost 3    three hops
% before(b,t), an ASP constraint over reachable_from/2, rules out s -> c -> t;
% maximum_cost(5), here the clingcon &sum bound, rules out s -> d -> b -> t. So
% the two layers of the split each dispose of one route. Neither touches the two
% survivors, which both weigh 4, and the edge-count tie-breaker, which stays in
% ASP, decides. Without it, both survivors are optimal.

#include "../../../encodings/shortest-path/variant-04-clingcon.lp".

vertex(s; a; b; c; d; t).

edge(
    s,c,1,1;
    c,t,1,1;
    s,d,1,1;
    d,b,1,9;
    s,b,3,1;
    b,t,1,1;
    s,a,2,1;
    a,b,1,1
).

start(s).
end(t).

before(b,t).

maximum_cost(5).
