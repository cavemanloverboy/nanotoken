use core::ops::{AddAssign, SubAssign};

use bytemuck::{Pod, Zeroable};
use solana_nostd_entrypoint::NoStdAccountInfo4;
use solana_program::{log, program_error::ProgramError};

use crate::{error::NanoTokenError, utils::split_at_unchecked, TokenAccount};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct Transfer {
    pub amount: u64,
}

impl Transfer {
    pub fn from_data<'a>(data: &mut &'a [u8]) -> Result<&'a Transfer, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<Transfer>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via core::slice::split_at
            // so we can return an error instead of panicking.
            let (ix_data, rem) = unsafe { split_at_unchecked(data, IX_LEN) };
            *data = rem;
            Ok(bytemuck::try_from_bytes(ix_data)
                .map_err(|_| ProgramError::InvalidInstructionData)?)
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub fn size() -> usize {
        core::mem::size_of::<Self>()
    }
}

pub fn transfer(accounts: &[NoStdAccountInfo4], args: &Transfer) -> Result<usize, ProgramError> {
    // log::sol_log("transfer");
    let [from, to, owner, _rem @ ..] = accounts else {
        log::sol_log("transfer expecting [from, to, owner, .. ]");
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // this seems to cost 0 cus...
    // // Return early if transfering zero
    // if args.amount == 0 {
    //     return Ok(3);
    // }

    // Check that owner signed this
    if !owner.is_signer() {
        log::sol_log("from account owner must sign to transfer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Load from_account
    // perf note: unsafe { unwrap_unchecked } uses more cus...
    let mut from_data = from.try_borrow_mut_data().expect("first borrow won't fail");
    let from_account = TokenAccount::checked_load_mut(&mut from_data)?;

    // Check from_account balance
    if from_account.balance < args.amount {
        log::sol_log("insufficient balance");
        return Err(NanoTokenError::InsufficientTokenBalance.into());
    }

    // Check that the owner is correct
    // if from_account.owner != *owner.key() {
    // if pubkey_neq(&from_account.owner, owner.key()) {
    if solana_program::program_memory::sol_memcmp(
        from_account.owner.as_ref(),
        owner.key().as_ref(),
        32,
    ) != 0
    {
        log::sol_log("incorrect from_account owner");
        return Err(ProgramError::IllegalOwner);
    }

    // Load to_account
    let mut to_data = to
        .try_borrow_mut_data()
        .ok_or(NanoTokenError::DuplicateAccount)?;
    let to_account = TokenAccount::checked_load_mut(&mut to_data)?;

    // Transfer
    from_account.balance -= args.amount;
    to_account.balance += args.amount;

    Ok(3)
}
