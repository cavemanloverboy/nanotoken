use bytemuck::{Pod, Zeroable};
use solana_nostd_entrypoint::NoStdAccountInfo4;
use solana_program::{log, program_error::ProgramError};

use crate::{error::NanoTokenError, utils::split_at_unchecked, Mint, TokenAccount};

#[derive(PartialEq, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct MintArgs {
    pub amount: u64,
}

impl MintArgs {
    pub fn from_data<'a>(data: &mut &'a [u8]) -> Result<&'a MintArgs, ProgramError> {
        const IX_LEN: usize = core::mem::size_of::<MintArgs>();
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

pub fn mint(accounts: &[NoStdAccountInfo4], args: &MintArgs) -> Result<usize, ProgramError> {
    log::sol_log("mint");
    let [to, mint, auth, _rem @ ..] = accounts else {
        log::sol_log("mint expecting [to, mint, auth, .. ]");
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Early return if 0
    if args.amount == 0 {
        return Ok(3);
    }

    // Load mint account
    // we do not do an owner check since we will mutate (add nonzero amount to supply)
    let mut mint_data = mint.try_borrow_mut_data().expect("first borrow won't fail"); // TODO unchecked
    let mint_account = Mint::checked_load_mut(&mut mint_data)?;

    // Check if auth is signer
    if !auth.is_signer() {
        log::sol_log("authority must sign to mint");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Check if auth is correct
    if mint_account.authority != *auth.key() {
        log::sol_log("incorrect mint authority");
        return Err(ProgramError::MissingRequiredSignature);
    };

    // Load account
    // we do not do an owner check since we will mutate (add nonzero amount to supply)
    let mut to_data = to
        .try_borrow_mut_data()
        .ok_or(NanoTokenError::DuplicateAccount)?;
    let to_account = TokenAccount::checked_load_mut(&mut to_data)?;

    // Increment supply, balance
    mint_account.supply += args.amount;
    to_account.balance += args.amount;

    Ok(3)
}
