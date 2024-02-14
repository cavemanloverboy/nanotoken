use bytemuck::{Pod, Zeroable};
use solana_nostd_entrypoint::NoStdAccountInfo4;
use solana_program::{log, program_error::ProgramError};

use crate::{error::NanoTokenError, utils::split_at_unchecked, TokenAccount};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TransferArgs {
    pub amount: u64,
}

impl TransferArgs {
    #[inline(always)]
    pub fn from_data<'a>(
        data: &mut &'a [u8],
    ) -> Result<&'a TransferArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<TransferArgs>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via
            // core::slice::split_at so we can return an error
            // instead of panicking.
            let (ix_data, rem) = unsafe { split_at_unchecked(data, IX_LEN) };
            *data = rem;

            // This is always aligned and all bit patterns are valid
            Ok(unsafe { &*(ix_data.as_ptr() as *const TransferArgs) })
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    #[inline(always)]
    pub fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

pub fn transfer(
    accounts: &[NoStdAccountInfo4],
    args: &TransferArgs,
) -> Result<usize, ProgramError> {
    // log::sol_log("transfer");
    // TODO DOCS
    let [from, to, owner, _rem @ ..] = accounts else {
        log::sol_log("transfer expecting [from, to, owner, .. ]");
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Return early if transfering zero
    //
    // This is necessary!
    // It is extremely cheap implicit owner check for from/to in nontrivial from
    // != to case. In the trivial from == to case, it doesn't matter since
    // nothing is transferred
    if args.amount == 0 {
        return Ok(3);
    }

    // Check that owner signed this
    if !owner.is_signer() {
        log::sol_log("from account owner must sign to transfer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Load from_account
    let (from_owner, from_mint, from_balance) =
        unsafe { TokenAccount::check_disc(from)? };
    let (_to_owner, to_mint, to_balance) =
        unsafe { TokenAccount::check_disc(to)? };

    // Check from_account balance
    if unsafe { *from_balance } < args.amount {
        log::sol_log("insufficient balance");
        return Err(NanoTokenError::InsufficientTokenBalance.into());
    }

    // Check that the owner is correct
    if solana_program::program_memory::sol_memcmp(
        from_owner.as_ref(),
        owner.key().as_ref(),
        32,
    ) != 0
    {
        log::sol_log("incorrect from_account owner");
        return Err(ProgramError::IllegalOwner);
    }

    // Check that the mints match
    if from_mint != to_mint {
        log::sol_log("from/to mint mismatch");
        return Err(NanoTokenError::IncorrectMint.into());
    }

    // Transfer
    unsafe {
        *from_balance -= args.amount;
        *to_balance += args.amount;
    }

    Ok(3)
}
