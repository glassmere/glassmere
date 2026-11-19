//! Every scenario of the library, timed.
//!
//! ```sh
//! cargo bench -p glassmere-bench                      # all of them
//! cargo bench -p glassmere-bench -- form/             # the ones that match
//! cargo bench -p glassmere-bench -- --save-baseline before
//! cargo bench -p glassmere-bench -- --baseline before # compare with it
//! ```
use criterion::{Criterion, criterion_group, criterion_main};

fn frames(criterion: &mut Criterion) {
    for scenario in glassmere_bench::scenarios() {
        let mut step = (scenario.build)();
        criterion.bench_function(scenario.name, |bencher| bencher.iter(&mut step));
    }
}

criterion_group!(benches, frames);
criterion_main!(benches);
