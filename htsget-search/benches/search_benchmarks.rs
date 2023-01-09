mod flamegraphs;

use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, black_box, BenchmarkGroup, Criterion};
use tokio::runtime::Runtime;

use htsget_config::config::cors::CorsConfig;
use htsget_config::Class::Header;
use htsget_config::Format::{Bam, Bcf, Cram, Vcf};
use htsget_config::Query;
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::htsget::HtsGet;
use htsget_search::htsget::HtsGetError;
use htsget_search::storage::data_server::HttpTicketFormatter;

const BENCHMARK_DURATION_SECONDS: u64 = 30;
const NUMBER_OF_SAMPLES: usize = 50;

async fn perform_query(query: Query) -> Result<(), HtsGetError> {
  let htsget = HtsGetFromStorage::local_from(
    "../data",
    HttpTicketFormatter::new(
      "127.0.0.1:8081".parse().expect("expected valid address"),
      CorsConfig::default(),
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

fn criterion_flamegraph(c: &mut Criterion) {
  c.bench_function("flamegraph_bam_query_all", |b| b.iter(|| Query::new("bam/htsnexus_test_NA12878", Bam)));
}

// fn criterion_flamegraph(c: &mut Criterion) {
//   //let bencher_func = Query::new(black_box("bam/htsnexus_test_NA12878"), Bam);

//   let mut group = c
//     .bench_function("bam_query_all_flamegraph",  bench_func)
//     .with_profiler(flamegraphs::FlamegraphProfiler::new(100))
//     .benchmark_group("Flamegraphs")
//     .sample_size(NUMBER_OF_SAMPLES)
//     .measurement_time(Duration::from_secs(BENCHMARK_DURATION_SECONDS));
// }

//criterion_group!(benches, criterion_benchmark);
// criterion_group!(benches, criterion_flamegraph);
criterion_group!{
		name = benches;
		config = Criterion::default().with_profiler(flamegraphs::FlamegraphProfiler::new(100));
		targets = criterion_flamegraph
}
criterion_main!(benches);