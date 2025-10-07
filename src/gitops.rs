use anyhow::Result;

use crate::cli::{BranchCreate, BranchFinalize, ReleasePr};

pub fn branch_create(args: &BranchCreate) -> Result<()> {
    let _ = args;
    // TODO: execute git workflow for branch creation.
    Ok(())
}

pub fn branch_finalize(args: &BranchFinalize) -> Result<()> {
    let _ = args;
    // TODO: execute git workflow for finalizing a branch.
    Ok(())
}

pub fn release_pr(args: &ReleasePr) -> Result<()> {
    let _ = args;
    // TODO: assemble changelog and open PR via gh.
    Ok(())
}
