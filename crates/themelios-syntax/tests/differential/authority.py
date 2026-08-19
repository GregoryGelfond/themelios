"""The authority's reading of one program (docs/design/syntax.md §16):
the program arrives on stdin, and one JSON object leaves on stdout —
the clingo version this process runs, whether the parser accepted the
program, and the statements it built, each as its AST type and the
authority's own printing of it. Test-only: run by tests/differential.rs
under the pixi environment; never shipped, never imported by anything.

The authority resolves `#include` from the working directory, which the
caller sets to the input's own directory; an include it cannot open is
a syntax error to it, reported here as `include_failed` so the caller
can tell that from a disagreement about the language. clingo carries
the "file could not be opened" detail on the diagnostic logger, not on
the RuntimeError it then raises, so a logger callback collects the
messages and the flag is read from them.
"""

import json
import sys

import clingo
from clingo.ast import parse_string


def read(program: str) -> dict:
    statements: list[dict] = []
    messages: list[str] = []
    try:
        parse_string(
            program,
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
            "version": clingo.__version__,
            "accepted": False,
            "message": message,
            "include_failed": opened in message or any(opened in m for m in messages),
        }
    return {"version": clingo.__version__, "accepted": True, "statements": statements}


if __name__ == "__main__":
    json.dump(read(sys.stdin.read()), sys.stdout)
