%! a documented rule
p(X) :- q(X), not r(X), X = 1..3, #sum { W,T : t(T,W) } >= 4.
