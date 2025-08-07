use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, criterion_group, criterion_main};
use http::uri::Authority;
use tokio::runtime::Runtime;

use htsget_config::resolver::ResolveResponse;
use htsget_config::storage;
use htsget_config::types::Class::Header;
use htsget_config::types::Format::{Bam, Bcf, Cram, Vcf};
use htsget_config::types::{HtsGetError, Query, Scheme};
use htsget_search::from_storage::HtsGetFromStorage;

const BENCHMARK_DURATION_SECONDS: u64 = 30;
const NUMBER_OF_SAMPLES: usize = 50;

async fn perform_query(query: Query) -> Result<(), HtsGetError> {
  HtsGetFromStorage::from_file(
    &storage::file::File::new(
      Scheme::Http,
      Authority::from_static("127.0.0.1:8081"),
      "../data".to_string(),
    ),
    &query,
  )
  .await?;

  Ok(())
}

fn bench_query(group: &mut BenchmarkGroup<WallTime>, name: &str, query: Query) {
  group.bench_with_input(name, &query, |b, input| {
    b.to_async(Runtime::new().unwrap())
      .iter(|| perform_query(input.clone()))
  });
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Queries");
  group
    .sample_size(NUMBER_OF_SAMPLES)
    .measurement_time(Duration::from_secs(BENCHMARK_DURATION_SECONDS));

  bench_query(
    &mut group,
    "[LIGHT] Bam query all",
    Query::new_with_default_request("bam/htsnexus_test_NA12878", Bam),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bam query specific",
    Query::new_with_default_request("bam/htsnexus_test_NA12878", Bam)
      .with_reference_name("11")
      .with_start(4999977)
      .with_end(5008321),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bam query header",
    Query::new_with_default_request("bam/htsnexus_test_NA12878", Bam).with_class(Header),
  );

  bench_query(
    &mut group,
    "[LIGHT] Cram query all",
    Query::new_with_default_request("cram/htsnexus_test_NA12878", Cram),
  );
  bench_query(
    &mut group,
    "[LIGHT] Cram query specific",
    Query::new_with_default_request("cram/htsnexus_test_NA12878", Cram)
      .with_reference_name("11")
      .with_start(4999977)
      .with_end(5008321),
  );
  bench_query(
    &mut group,
    "[LIGHT] Cram query header",
    Query::new_with_default_request("cram/htsnexus_test_NA12878", Cram).with_class(Header),
  );

  bench_query(
    &mut group,
    "[LIGHT] Vcf query all",
    Query::new_with_default_request("vcf/sample1-bcbio-cancer", Vcf),
  );
  bench_query(
    &mut group,
    "[LIGHT] Vcf query specific",
    Query::new_with_default_request("vcf/sample1-bcbio-cancer", Vcf)
      .with_reference_name("chrM")
      .with_start(151)
      .with_end(153),
  );
  bench_query(
    &mut group,
    "[LIGHT] Vcf query header",
    Query::new_with_default_request("vcf/sample1-bcbio-cancer", Vcf).with_class(Header),
  );

  bench_query(
    &mut group,
    "[LIGHT] Bcf query all",
    Query::new_with_default_request("bcf/sample1-bcbio-cancer", Bcf),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bcf query specific",
    Query::new_with_default_request("bcf/sample1-bcbio-cancer", Bcf)
      .with_reference_name("chrM")
      .with_start(151)
      .with_end(153),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bcf query header",
    Query::new_with_default_request("bcf/sample1-bcbio-cancer", Bcf).with_class(Header),
  );

  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
