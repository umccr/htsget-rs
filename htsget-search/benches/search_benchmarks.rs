use std::net::SocketAddr;
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use tokio::runtime::Runtime;

use htsget_config::regex_resolver::RegexResolver;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::htsget::Class::Header;
use htsget_search::htsget::Format::{Bam, Bcf, Cram, Vcf};
use htsget_search::htsget::HtsGet;
use htsget_search::htsget::{HtsGetError, Query};
use htsget_search::storage::axum_server::HttpsFormatter;

const BENCHMARK_DURATION_SECONDS: u64 = 15;
const NUMBER_OF_SAMPLES: usize = 150;

async fn perform_query(query: Query) -> Result<(), HtsGetError> {
  let htsget = HtsGetFromStorage::local_from(
    "../data",
    RegexResolver::new(".*", "$0").unwrap(),
    HttpsFormatter::from(
      "127.0.0.1:8081"
        .parse::<SocketAddr>()
        .expect("Expected valid address."),
    ),
  )?;

  htsget.search(query).await?;
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
    Query::new("bam/htsnexus_test_NA12878", Bam),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bam query specific",
    Query::new("bam/htsnexus_test_NA12878", Bam)
      .with_reference_name("11")
      .with_start(4999977)
      .with_end(5008321),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bam query header",
    Query::new("bam/htsnexus_test_NA12878", Bam).with_class(Header),
  );

  bench_query(
    &mut group,
    "[LIGHT] Cram query all",
    Query::new("cram/htsnexus_test_NA12878", Cram),
  );
  bench_query(
    &mut group,
    "[LIGHT] Cram query specific",
    Query::new("cram/htsnexus_test_NA12878", Cram)
      .with_reference_name("11")
      .with_start(4999977)
      .with_end(5008321),
  );
  bench_query(
    &mut group,
    "[LIGHT] Cram query header",
    Query::new("cram/htsnexus_test_NA12878", Cram).with_class(Header),
  );

  bench_query(
    &mut group,
    "[LIGHT] Vcf query all",
    Query::new("vcf/sample1-bcbio-cancer", Vcf),
  );
  bench_query(
    &mut group,
    "[LIGHT] Vcf query specific",
    Query::new("vcf/sample1-bcbio-cancer", Vcf)
      .with_reference_name("chrM")
      .with_start(151)
      .with_end(153),
  );
  bench_query(
    &mut group,
    "[LIGHT] Vcf query header",
    Query::new("vcf/sample1-bcbio-cancer", Vcf).with_class(Header),
  );

  bench_query(
    &mut group,
    "[LIGHT] Bcf query all",
    Query::new("bcf/sample1-bcbio-cancer", Bcf),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bcf query specific",
    Query::new("bcf/sample1-bcbio-cancer", Bcf)
      .with_reference_name("chrM")
      .with_start(151)
      .with_end(153),
  );
  bench_query(
    &mut group,
    "[LIGHT] Bcf query header",
    Query::new("bcf/sample1-bcbio-cancer", Bcf).with_class(Header),
  );

  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
