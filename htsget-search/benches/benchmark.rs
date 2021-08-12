use criterion::{criterion_group, criterion_main, Criterion};
use htsget_search::{
  htsget::{
    from_storage::HtsGetFromStorage, Class, Fields, Format, HtsGet, HtsGetError, Query, Tags,
  },
  storage::local::LocalStorage,
};
use std::time::Duration;

const BENCHMARK_DURATION_SECONDS: u64 = 5;
const NUMBER_OF_EXECUTIONS: usize = 150;

fn perform_query(query: Query) -> Result<(), HtsGetError> {
  let htsget = HtsGetFromStorage::new(LocalStorage::new("../data", "localhost").unwrap());
  htsget.search(query)?;
  Ok(())
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Queries");
  group
    .sample_size(NUMBER_OF_EXECUTIONS)
    .measurement_time(Duration::from_secs(BENCHMARK_DURATION_SECONDS));

  group.bench_function("[LIGHT] Simple bam query", |b| {
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
  group.bench_function("[LIGHT] Bam query", |b| {
    b.iter(|| {
      perform_query(Query {
        id: "bam/htsnexus_test_NA12878".to_string(),
        format: None,
        class: Class::Body,
        reference_name: Some("11".to_string()),
        start: Some(4999977),
        end: Some(5008321),
        fields: Fields::All,
        tags: Tags::All,
        no_tags: None,
      })
    })
  });
  group.bench_function("[LIGHT] VCF query", |b| {
    b.iter(|| {
      perform_query(Query {
        id: "vcf/sample1-bcbio-cancer".to_string(),
        format: None,
        class: Class::Body,
        reference_name: Some("chrM".to_string()),
        start: Some(151),
        end: Some(153),
        fields: Fields::All,
        tags: Tags::All,
        no_tags: None,
      })
    })
  });
  group.bench_function("[LIGHT] BCF query", |b| {
    b.iter(|| {
      perform_query(Query {
        id: "bcf/sample1-bcbio-cancer".to_string(),
        format: Some(Format::Bcf),
        class: Class::Body,
        reference_name: Some("chrM".to_string()),
        start: Some(151),
        end: Some(153),
        fields: Fields::All,
        tags: Tags::All,
        no_tags: None,
      })
    })
  });
  group.bench_function("[LIGHT] CRAM query", |b| {
    b.iter(|| {
      perform_query(Query {
        id: "cram/htsnexus_test_NA12878".to_string(),
        format: Some(Format::Cram),
        class: Class::Body,
        reference_name: Some("11".to_string()),
        start: Some(4999977),
        end: Some(5008321),
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
