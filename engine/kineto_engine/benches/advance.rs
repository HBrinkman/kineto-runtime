use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kineto_engine::player::KinetoPlayer;

fn bench_advance_animated_sample(c: &mut Criterion) {
    let bytes = include_bytes!("../tests/fixtures/animated_sample.kvk").to_vec();
    let mut player = KinetoPlayer::load(bytes).expect("fixture should load");
    player.play().expect("fixture should produce an initial frame");

    c.bench_function("advance_animated_sample_frame", |b| {
        b.iter(|| {
            black_box(
                player
                    .advance(black_box(1.0 / 60.0))
                    .expect("frame should advance"),
            );
        });
    });
}

criterion_group!(benches, bench_advance_animated_sample);
criterion_main!(benches);
