% N-Queens (N = 8), clingcon constraint model.
% @expect sat
% @count  92
% @note   clingcon's observable is the per-row column assignment q(R)=C, not queen_at/2; the shared invariant with the ASP variants is the 92-solution count
% @elenctic solver clingcon
%
% The same 92 solutions as the ASP variants, recast for clingcon's CP backend.
% One integer variable per row holds the column of that row's queen; an
% all-different constraint on those columns, plus one on each diagonal
% direction, rules out every attack. The whole board reduces to three &distinct
% constraints, where the ASP variants spell the same conditions out over
% queen_at/2.

#const n = 8.

row(1..n).

% q(R): the column of the queen in row R, a CSP integer in [1, n].
&dom { 1..n } = q(R) :- row(R).

% Distinct columns (no two queens in a column), and distinct diagonals in each
% direction: two queens attack along "\" when q(R) - R agrees, along "/" when
% q(R) + R agrees, so both sets must be all-different.
&distinct { q(R) : row(R) }.
&distinct { q(R)-R : row(R) }.
&distinct { q(R)+R : row(R) }.

#show.
