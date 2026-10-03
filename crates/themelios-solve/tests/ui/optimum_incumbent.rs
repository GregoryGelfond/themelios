// A step of the improving trajectory is an incumbent — a best-found model,
// typed apart from the proven optimum — with no public constructor, so no
// client can fabricate one (docs/design/solve.md §5.2, §5.3): its door lands
// with `optimize`'s. The refusal is the compile error held beside this file.
use themelios_solve::outcome::{Incumbent, Model};

fn main() {
    let _ = Incumbent {
        model: Model::of(Default::default()),
    };
}
