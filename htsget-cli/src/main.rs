use anyhow::Result;
use clap::ArgMatches; //{App, Arg, ArgMatches, SubCommand};

use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::{
  htsget::{HtsGet, Query}, //, Response},
  storage::local::LocalStorage,
};

fn main() -> Result<()> {
  let _storage = LocalStorage::new("../data")?;
  // let htsget = HtsGetFromStorage::new(storage);
  // let args = unimplemented!();
  // htsget_search(&mut htsget, args)?;
  unimplemented!()
}

fn htsget_search<HG>(htsget: &mut HG, _args: &ArgMatches) -> Result<()>
where
  HG: HtsGet,
{
  // let id = args.value_of("id").unwrap().to_string();
  // TODO build the Query from the args ...

  let query = Query::new("BroadHiSeqX_b37/NA12878")
    .with_reference_name("11")
    .with_start(5011963)
    .with_end(5012660);

  println!("Searching {:#?}: ", query);

  let response = htsget.search(query)?;

  println!("{:#?}", response);

  Ok(())
}
