use std::path::{Path, PathBuf};

use crate::unredact_orchestrator::logic::{
    run_orchestrator, OrchestratorConfig, OrchestratorOutputs, OrchestratorRequest,
};

#[inline]
pub fn run_from_paths(
    input: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: OrchestratorConfig,
) -> Result<OrchestratorOutputs, String> {
    let req = OrchestratorRequest {
        input: input.to_path_buf(),
        output_dir: output_dir.to_path_buf(),
        dictionary_path: dictionary_path.map(PathBuf::from),
        cfg,
    };

    run_orchestrator(req)
}
