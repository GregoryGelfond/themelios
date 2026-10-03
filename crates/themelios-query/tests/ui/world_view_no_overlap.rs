// Holding a `members` stream borrows the world view mutably (docs/design/query.md
// §2.3), so a second overlapping read cannot even be written: the borrow checker is
// the serialisation, not a run-time re-entrancy check. The error held beside this
// file proves a second `members` call while the first stream is live does not
// compile.
use themelios_program::program::Program;
use themelios_solve::agent::{Agent, Scenario};
use themelios_solve::bridge::Door;
use themelios_solve::contract::{Backend, Capabilities, Fault, SolveRequest};
use themelios_solve::outcome::{AnswerSet, Conclusion, Determination, Model, Run, ShowRule, Solved};
use themelios_query::WorldView;

// A backend with one model over a closed space — a consistent world view.
struct OneModel;

struct Once {
    sets: std::vec::IntoIter<AnswerSet>,
    ended: bool,
}

impl Run for Once {
    fn next_model(&mut self) -> Option<Result<Model, Fault>> {
        if let Some(set) = self.sets.next() {
            Some(Ok(Model::of(set)))
        } else {
            self.ended = true;
            None
        }
    }

    fn conclusion(&self) -> Option<Conclusion> {
        self.ended.then_some(Conclusion::Exhausted)
    }
}

impl Backend for OneModel {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn solve(&mut self, _request: &SolveRequest) -> Result<Solved<'_>, Fault> {
        Ok(Solved::running(
            Box::new(Once {
                sets: vec![AnswerSet::new()].into_iter(),
                ended: false,
            }),
            Scenario::default(),
ShowRule::default(),
        ))
    }

    fn lower(&mut self, _door: Door<'_>) -> Result<(), Fault> {
        Ok(())
    }
}

fn main() {
    let mut agent = Agent::new(Program::empty(), OneModel);
    let Ok(Determination::Consistent(models)) = agent.determination() else {
        return;
    };
    let mut world_view = WorldView::of(models);
    let first = world_view.members();
    let second = world_view.members();
    let _ = (first, second);
}
