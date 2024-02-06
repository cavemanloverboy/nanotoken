extern crate alloc; // needed for rkyv args

use bytemuck::{Pod, Zeroable};
use solana_nostd_entrypoint::NoStdAccountInfo4;
use solana_program::{entrypoint::ProgramResult, log, program_error::ProgramError};

use crate::{
    utils::{split_at_mut_unchecked, split_at_unchecked},
    AccountDiscriminator, ProgramConfig,
};

#[derive(PartialEq, Debug, Clone, Pod, Zeroable, Copy)]
#[repr(C)]
pub struct InitConfigArgs {
    // Keeping this scaffolded just in case...
}

impl InitConfigArgs {
    pub fn from_data<'a>(data: &mut &'a [u8]) -> Result<&'a InitConfigArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<InitConfigArgs>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via core::slice::split_at
            // so we can return an error instead of panicking.
            let (ix_data, rem) = unsafe { split_at_unchecked(data, IX_LEN) };
            *data = rem;

            Ok(unsafe { &*(ix_data.as_ptr() as *const InitConfigArgs) })
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }
}

pub fn initialize_config(
    accounts: &[NoStdAccountInfo4],
    args: &InitConfigArgs,
) -> Result<usize, ProgramError> {
    log::sol_log("initializing config");
    // Unpack accounts
    //
    // 1) Config account needs a pubkey + owner + data_len check, which is done in
    //    checked_initialize_config
    // 2) unused
    // 3) Anyone can be the initializer because initialization is determinstic with no args,
    //    so they're basically just paying tx fee
    let [_rem @ .., config, _system_program, _payer] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // 3) Config account needs a pubkey + owner + data_len check, which is done in
    //    checked_initialize_config
    checked_initialize_config(config, args)?;

    Ok(0)
}

/// Checks program config and initializes it
///
/// Check 1) Expecting a particular pubkey (done in process_instruction)
/// Check 2) Expecting a particular data length
/// Check 3) Expecting uninitialized disc
///
/// Init 1) Write initialized disc
/// Init 2) Write config
///
/// Note: owner check is done by the runtime after we validate data change.
/// If we vailidate uninitialize disc, write initialized disc, and then
/// the runtime complains, then we were not the account owner.
fn checked_initialize_config(config: &NoStdAccountInfo4, _args: &InitConfigArgs) -> ProgramResult {
    // Get account data
    // SAFETY: this is the one and only time any account data is mutably borrowed
    //         in this instruction
    let config_account_data = unsafe { config.unchecked_borrow_mut_data() };

    // Check 2) Expecting a particular data length
    if config_account_data.len() != ProgramConfig::size() + 8 {
        log::sol_log("config data len is incorrect");
        return Err(ProgramError::InvalidAccountData);
    }

    // Split 8-byte (padded) aligned discriminator + config
    // SAFETY:
    // We manually checked length above to return error instead of panicking, so we
    // do not need to do any bounds checks.
    unsafe {
        let (padded_disc, config_data) = split_at_mut_unchecked(config_account_data, 8);

        // Check 3) Expecting uninitialized disc
        if *padded_disc.get_unchecked(0) != AccountDiscriminator::Unintialized as u8 {
            log::sol_log("config was already initialized");
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        // Init 1) Write initialized disc
        *padded_disc.get_unchecked_mut(0) = AccountDiscriminator::Config as u8;

        // Init 2) Write config
        // Note:
        // This deconstruction pattern future proofs initialization for new fields
        // SAFETY: due to 8 byte disc and bpf alignment, size and 8-byte alignment of bytes is checked
        const _: () = assert!(core::mem::align_of::<ProgramConfig>() == 8);
        let ProgramConfig { mint_index } = &mut *(config_data.as_mut_ptr() as *mut ProgramConfig);
        *mint_index = 0;
    }
    Ok(())
}
