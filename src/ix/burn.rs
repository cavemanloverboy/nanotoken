use bytemuck::{Pod, Zeroable};
use solana_nostd_entrypoint::NoStdAccountInfo4;
use solana_program::{log, program_error::ProgramError};

use crate::{
    error::NanoTokenError, utils::split_at_unchecked, Mint, TokenAccount,
};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct BurnArgs {
    pub amount: u64,
}

impl BurnArgs {
    pub fn from_data<'a>(
        data: &mut &'a [u8],
    ) -> Result<&'a BurnArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<BurnArgs>();
        if data.len() >= IX_LEN {
            // SAFETY:
            // We do the length check ourselves instead of via
            // core::slice::split_at so we can return an error
            // instead of panicking.
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

pub fn burn(
    accounts: &[NoStdAccountInfo4],
    args: &BurnArgs,
) -> Result<usize, ProgramError> {
    log::sol_log("burn");
    let [from, mint, owner, _rem @ ..] = accounts else {
        log::sol_log("mint expecting [from, mint, owner, .. ]");
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Early return if 0
    // this seems to cost 0 cus...
    //
    // This is necessary!
    // It is extremely cheap implicit owner check for mint/to
    if args.amount == 0 {
        return Ok(3);
    }

    // Load mint account
    // we do not do an owner check since we will mutate (add nonzero amount to
    // supply)
    let mut mint_data = mint
        .try_borrow_mut_data()
        .expect("first borrow won't fail"); // TODO unchecked
    let mint_account = Mint::checked_load_mut(&mut mint_data)?;

    // Check if from is signer
    if !from.is_signer() {
        log::sol_log("authority must sign to mint");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Load account
    // we do not do an owner check since we will mutate (sub nonzero amount from
    // supply/balance)
    let mut from_data = from
        .try_borrow_mut_data()
        .ok_or(NanoTokenError::DuplicateAccount)?;
    let from_account = TokenAccount::checked_load_mut(&mut from_data)?;

    // Check mint
    if from_account.mint != mint_account.mint_index {
        log::sol_log("invalid mint");
        return Err(NanoTokenError::IncorrectMint.into());
    }

    // Check if owner is correct
    if solana_program::program_memory::sol_memcmp(
        from_account.owner.as_ref(),
        owner.key().as_ref(),
        32,
    ) != 0
    {
        log::sol_log("incorrect mint authority");
        return Err(ProgramError::MissingRequiredSignature);
    };

    // Check balance
    if from_account.balance >= args.amount {
        // decrement supply, balance
        mint_account.supply -= args.amount;
        from_account.balance -= args.amount;
    } else {
        log::sol_log("insufficient token balance");
        return Err(NanoTokenError::InsufficientTokenBalance.into());
    }

    Ok(3)
}
