use crate::solana_nostd_entrypoint::NoStdAccountInfo;
use bytemuck::{Pod, Zeroable};
use solana_program::{
    entrypoint::ProgramResult, log, program_error::ProgramError, pubkey::Pubkey,
};

use crate::{
    error::NanoTokenError,
    utils::{split_at_mut_unchecked, split_at_unchecked},
    AccountDiscriminator, Mint, ProgramConfig,
};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct InitializeMintArgs {
    pub authority: Pubkey,
    /// u64 is used for alignment. Max value is 12
    pub decimals: u64,
}

impl InitializeMintArgs {
    pub fn from_data<'a>(
        data: &mut &'a [u8],
    ) -> Result<&'a InitializeMintArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<InitializeMintArgs>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via
            // core::slice::split_at so we can return an error
            // instead of panicking.
            let (ix_data, rem) = unsafe { split_at_unchecked(data, IX_LEN) };
            *data = rem;

            // This is always aligned and all bit patterns are valid
            Ok(unsafe { &*(ix_data.as_ptr() as *const InitializeMintArgs) })
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub const fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

pub fn initialize_mint(
    accounts: &[NoStdAccountInfo],
    args: &InitializeMintArgs,
) -> Result<usize, ProgramError> {
    log::sol_log("init mint");
    // Unpack accounts
    //
    // 1) config is checked by ProgramConfig::unchecked_load
    // 2) Mint account needs a owner + data_len check, which is done in
    //   checked_initialize_mint. Mint is signer
    let [mint, _rem @ .., config, _sys, _payer] = accounts else {
        log::sol_log("expecting mint, .. config, system_program, payer");
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    checked_initialized_mint(config, mint, &args.authority, &args.decimals)?;

    Ok(1)
}

/// Checks mint account and initializes it
///
/// Check 1) Expecting a particular data length
/// Check 2) Expecting uninitialized disc
///
/// Init 1) Write initialized disc
/// Init 2) Write initial state
///
/// /// Note: owner check is done by the runtime after we validate data change.
/// If we validate uninitialized disc, write initialized disc, and then
/// the runtime complains, then we were not the account owner.
pub(crate) fn checked_initialized_mint(
    config: &NoStdAccountInfo,
    mint: &NoStdAccountInfo,
    mint_authority: &Pubkey,
    mint_decimals: &u64,
) -> ProgramResult {
    // Get account data
    // SAFETY: this is the one and only time any account data is mutably
    // borrowed in this instruction
    let mint_account_data = unsafe { mint.unchecked_borrow_mut_data() };

    // Check 1) Expecting a particular data length
    if mint_account_data.len() != Mint::size() + 8 {
        log::sol_log("mint data len is incorrect");
        return Err(ProgramError::InvalidAccountData);
    }

    // Get the mint index for this mint account, and increment index in config
    let this_mint_index = {
        // 2) config is checked by ProgramConfig::unchecked_load
        let config_account =
            unsafe { ProgramConfig::unchecked_load_mut(config)? };
        let idx = config_account.mint_index;
        config_account.mint_index += 1;
        idx
    };

    // Split 8-byte (padded) aligned discriminator + config
    // SAFETY:
    // We manually checked length above to return error instead of panicking, so
    // we do not need to do any bounds checks.
    unsafe {
        let (padded_disc, config_data) =
            split_at_mut_unchecked(mint_account_data, 8);

        // Check 3) Expecting uninitialized disc
        if *padded_disc.get_unchecked(0)
            != AccountDiscriminator::Unintialized as u8
        {
            log::sol_log("config was already initialized");
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        // Init 1) Write initialized disc
        *padded_disc.get_unchecked_mut(0) = AccountDiscriminator::Mint as u8;

        // Init 2) Write config
        // Note:
        // This deconstruction pattern future proofs initialization for new
        // fields SAFETY: due to 8 byte disc and bpf alignment, size and
        // 8-byte alignment of bytes is checked
        //
        // Initial supply is zero.
        const _: () = assert!(core::mem::align_of::<Mint>() == 8);
        let Mint {
            authority,
            supply,
            decimals,
            mint_index,
            _padding,
        } = &mut *(config_data.as_mut_ptr() as *mut Mint);
        *mint_index = this_mint_index;
        *authority = *mint_authority;
        *supply = 0;
        if *mint_decimals > 12 {
            log::sol_log("max decimals is 12");
            return Err(NanoTokenError::InvalidDecimals.into());
        }
        *decimals = *mint_decimals as u8;
    }

    Ok(())
}
