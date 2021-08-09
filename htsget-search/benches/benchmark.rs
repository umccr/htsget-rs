use criterion::{criterion_group, criterion_main, Criterion};
use htsget_search::{
  htsget::{from_storage::HtsGetFromStorage, Class, Fields, HtsGet, HtsGetError, Query, Tags},
  storage::local::LocalStorage,
};
use std::time::Duration;

fn perform_query(query: Query) -> Result<(), HtsGetError> {
  let htsget = HtsGetFromStorage::new(LocalStorage::new("../data", "localhost").unwrap());
  htsget.search(query)?;
  Ok(())
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Queries");
  group
    .sample_size(20)
    .measurement_time(Duration::from_secs(5));

  group.bench_function("Simple bam query", |b| {
    b.iter(|| {
      perform_query(Query {
        id: "bam/htsnexus_test_NA12878".to_string(),
        format: None,
        class: Class::Body,
        reference_name: None,
        start: None,
        end: None,
        fields: Fields::All,
        tags: Tags::All,
        no_tags: None,
      })
    })
  });

  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
