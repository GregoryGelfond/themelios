"""The authority's readings for the program and analysis differentials
(docs/design/program.md §16; docs/design/analysis.md §10; docs/grammar.md §3):
the pinned clingo 5.8.2, driven in one of four modes chosen by the first
argument, one JSON object leaving on stdout. Both tiers' tests/differential.rs
spawn this one driver — the program tier for `parse`, `eval`, and `order`, the
analysis tier for `safety`. Test-only: run under the pixi environment; never
shipped, never imported by anything.

- `parse`: the program arrives on stdin; the reply is the clingo version, whether
  the parser accepted it, and the statements it built — each as its AST type and
  the authority's own printing. An `#include` is resolved from the working
  directory, which the caller sets to the input's own directory; one the authority
  cannot open is a syntax error to it, reported as `include_failed` so the caller
  can tell that from a disagreement about the language. clingo carries the "file
  could not be opened" detail on the diagnostic logger, so a logger callback
  collects the messages and the flag is read from them.
- `eval`: a JSON object `{"terms": [...]}` of ground-term spellings arrives on
  stdin; the reply gives, per term, the symbol the authority evaluates it to (its
  printing, whether it is a number, and the number when it is) or the error when
  the authority refuses it. This is the authority's ground-term arithmetic — the
  same evaluation `parse_term` performs, so overflow wraps and division by zero
  refuses, exactly as the grounder does.
- `order`: a JSON object `{"symbols": [...]}` of ground-symbol spellings arrives on
  stdin; the reply gives them sorted by the authority's total term order, and — in
  the input's order — each spelling as the authority prints it, so the caller can
  confirm every spelling it sent is already the authority's own.
- `safety`: the program arrives on stdin; the authority grounds it and the reply
  says whether it is safe — the authority reports an unsafe variable on the
  diagnostic logger and stops grounding, so `safe` is the absence of that report.
"""

import json
import sys

import clingo
from clingo.ast import parse_string

VERSION = clingo.__version__


def read_parse() -> dict:
    """The authority's parse of the program on stdin (docs/design/program.md §16)."""
    statements: list[dict] = []
    messages: list[str] = []
    try:
        parse_string(
            sys.stdin.read(),
            lambda statement: statements.append(
                {"type": statement.ast_type.name, "text": str(statement)}
            ),
            logger=lambda code, message: messages.append(message),
            message_limit=100,
        )
    except RuntimeError as error:
        message = str(error)
        opened = "file could not be opened"
        return {
            "version": VERSION,
            "accepted": False,
            "message": message,
            "include_failed": opened in message or any(opened in m for m in messages),
        }
    return {
        "version": VERSION,
        "accepted": True,
        "include_failed": False,
        "statements": statements,
    }


def read_eval() -> dict:
    """The authority's evaluation of each ground term (docs/design/program.md §3.5)."""
    terms = json.load(sys.stdin)["terms"]
    results = []
    for term in terms:
        try:
            symbol = clingo.parse_term(term)
        except Exception as error:  # noqa: BLE001 — any refusal is "no value"
            results.append(
                {"term": term, "ok": False, "error": f"{type(error).__name__}: {error}"}
            )
            continue
        is_number = symbol.type == clingo.SymbolType.Number
        results.append(
            {
                "term": term,
                "ok": True,
                "symbol": str(symbol),
                "is_number": is_number,
                "number": symbol.number if is_number else None,
            }
        )
    return {"version": VERSION, "results": results}


def read_order() -> dict:
    """The authority's total order over the ground symbols (docs/design/program.md §3.1)."""
    spellings = json.load(sys.stdin)["symbols"]
    parsed = [(spelling, clingo.parse_term(spelling)) for spelling in spellings]
    ordered = sorted(parsed, key=lambda pair: pair[1])
    return {
        "version": VERSION,
        "sorted": [str(symbol) for _, symbol in ordered],
        "printed": [str(symbol) for _, symbol in parsed],
    }


def read_safety() -> dict:
    """Whether the authority grounds the program without an unsafe-variable report
    (docs/design/analysis.md §5, §10)."""
    program = sys.stdin.read()
    messages: list[str] = []
    control = clingo.Control(
        logger=lambda code, message: messages.append(message), message_limit=100
    )
    stopped = False
    try:
        control.add("base", [], program)
        control.ground([("base", [])])
    except RuntimeError:
        stopped = True
    unsafe = any("unsafe" in message for message in messages)
    return {
        "version": VERSION,
        "safe": not unsafe,
        "stopped": stopped,
        "messages": messages,
    }


MODES = {
    "parse": read_parse,
    "eval": read_eval,
    "order": read_order,
    "safety": read_safety,
}


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "parse"
    json.dump(MODES[mode](), sys.stdout)
