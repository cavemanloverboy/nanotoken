use crate::solana_nostd_entrypoint::NoStdAccountInfo;
use bytemuck::{Pod, Zeroable};
use solana_program::{
    entrypoint::ProgramResult, log, program_error::ProgramError, pubkey::Pubkey,
};

use crate::{
    utils::{
        create_pda_funded_by_payer, split_at_mut_unchecked, split_at_unchecked,
    },
    AccountDiscriminator, ProgramConfig, TokenAccount,
};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct InitializeAccountArgs {
    pub owner: Pubkey,
    pub mint: u64,
    // 8 byte alignment for good devex, otherwise need repr(packed) + unaligned reads/writes.
    //
    // This is provided as an argument to provide the option to do it off
    // chain. Otherwise, if we do it on-chain via a syscall, it will always
    // be done. The cpi client will abstract this away and do it internally
    pub bump: u64,
}

impl InitializeAccountArgs {
    pub fn from_data<'a>(
        data: &mut &'a [u8],
    ) -> Result<&'a InitializeAccountArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<InitializeAccountArgs>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via
            // core::slice::split_at so we can return an error
            // instead of panicking.
            let (ix_data, rem) = unsafe { split_at_unchecked(data, IX_LEN) };
            *data = rem;

            // This is always aligned and all bit patterns are valid
            Ok(unsafe { &*(ix_data.as_ptr() as *const InitializeAccountArgs) })
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub const fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

pub fn initialize_account(
    accounts: &[NoStdAccountInfo],
    args: &InitializeAccountArgs,
) -> Result<usize, ProgramError> {
    // log::sol_log("init account");
    // Unpack accounts
    //
    // 1) Token account will be checked by checked_initialize_account
    // 2) Config will be checked
    // 4) payer will be checked by the sol transfer if necessary
    let [token_account, _rem @ .., config, system_program, payer] = accounts
    else {
        log::sol_log(
            "expecting token_account, ... config, system_program, payer",
        );
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    checked_initialize_account(
        payer,
        config,
        token_account,
        system_program,
        &args.owner,
        args.mint,
        args.bump as u8,
    )?;

    Ok(1)
}

/// Creates token account and initializes it
///
/// Check 1) Check seeds (valid index + checked by initialization)
/// owner and data len need not be checked since we are allocating account
///
/// Init 1) Create token account
/// Init 2) Write initialized disc
/// Init 3) Write initial state
///
/// /// Note: owner check is done by the runtime after we validate data change.
/// If we validate uninitialized disc, write initialized disc, and then
/// the runtime complains, then we were not the account owner.
pub(crate) fn checked_initialize_account(
    payer: &NoStdAccountInfo,
    config: &NoStdAccountInfo,
    token_account: &NoStdAccountInfo,
    system_program: &NoStdAccountInfo,
    // args: &InitializeAccountArgs,
    account_owner: &Pubkey,
    account_mint: u64,
    bump: u8,
) -> ProgramResult {
    // Check 1) Check seeds (valid index + checked by initialization)
    let mint_index: [u8; 8] = {
        // SAFETY: no one else has a view into config data during this scope
        let config_account =
            unsafe { ProgramConfig::unchecked_load_mut(config)? };

        // If the mint provided is not than the current mint_index, this is a
        // valid mint
        if account_mint >= config_account.mint_index {
            log::sol_log("mint u64 provided for initialization is not valid");
            return Err(ProgramError::InvalidInstructionData);
        }

        account_mint.to_le_bytes()
    };
    let token_account_seeds: &[&[u8]] =
        &[account_owner.as_ref(), mint_index.as_ref(), &[bump]];

    // Init 1) Create token account
    create_pda_funded_by_payer(
        token_account.to_info_c(),
        &crate::ID,
        TokenAccount::space() as u64,
        token_account_seeds,
        system_program.to_info_c(),
        payer.to_info_c(),
    )?;

    // Split data into discriminator and token account
    // SAFETY:
    // 1) no one holds a view into the token account
    // 2) we just validated data length by creating account
    unsafe {
        let account_data = token_account.unchecked_borrow_mut_data();
        let (disc, token_account_data) =
            split_at_mut_unchecked(account_data, 8);

        // Init 2) Write initialized disc
        *(disc.as_mut_ptr() as *mut u8) = AccountDiscriminator::Token as u8;

        // Init 3) Write initial state
        let TokenAccount {
            owner,
            mint,
            balance,
        } = &mut *(token_account_data.as_mut_ptr() as *mut TokenAccount);
        *owner = *account_owner;
        // SAFETY: little endian byte memcpy. alignment is correct due to
        // TokenAccount.
        *(mint as *mut u64 as *mut [u8; 8]) = mint_index;
        *balance = 0;
    }

    Ok(())
}
