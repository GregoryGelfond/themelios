% SEND + MORE = MONEY (clingcon variant).
% @expect sat
% @count  1
% @assign { digit(s)=9, digit(e)=5, digit(n)=6, digit(d)=7, digit(m)=1, digit(o)=0, digit(r)=8, digit(y)=2 }
% @elenctic solver clingcon
%
% The same cryptarithmetic puzzle as send-money.lp, encoded natively in clingcon:
% each letter is a CSP integer in [0, 9], the leading digits are nonzero by &sum
% bounds, all-different is one &distinct, and SEND + MORE = MONEY is a single
% linear &sum equation. The CP backend solves it directly, so the column-by-column
% carry decomposition the pure-ASP encoding needs for grounding is unnecessary.
% Same unique solution: S=9, E=5, N=6, D=7, M=1, O=0, R=8, Y=2 (9567 + 1085 = 10652).

% Domain

letter(s; e; n; d; m; o; r; y).

% CSP variables

% digit(L): each letter as a CSP integer in [0, 9].
&dom { 0..9 } = digit(L) :- letter(L).

% Constraints (clingcon)

% Leading digits S and M are nonzero.
&sum { digit(s) } >= 1.
&sum { digit(m) } >= 1.

% All eight letters take distinct digits.
&distinct { digit(L) : letter(L) }.

% The identity SEND + MORE = MONEY, as a single linear equation
% (SEND + MORE - MONEY = 0).
&sum {
       1000*digit(s) +   100*digit(e) +  10*digit(n) +    digit(d)
   +   1000*digit(m) +   100*digit(o) +  10*digit(r) +    digit(e)
   -  10000*digit(m) -  1000*digit(o) - 100*digit(n) -  10*digit(e) - digit(y)
} = 0.

% Output

#show.
