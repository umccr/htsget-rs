use not_perf::profiler::ProfilingController;
use not_perf::args::GenericProfilerArgs;

fn main() {
    let pid = std::os::unix::process::parent_id(); // TODO: Locate benchmark pids and/or binaries?
    let prof_args = GenericProfilerArgs::new(pid.try_into().unwrap());
    let _prof_ctrl = ProfilingController::new(&prof_args);
}