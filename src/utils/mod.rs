use arrayref::mut_array_refs;
use solana_nostd_entrypoint::{AccountInfoC, InstructionC};
use solana_program::{
    entrypoint::ProgramResult, log, program_error::ProgramError, pubkey::Pubkey, rent::Rent,
    sysvar::Sysvar,
};

/// Creates a new pda
#[inline(always)]
pub fn create_pda_funded_by_payer(
    target_account: AccountInfoC,
    owner: &Pubkey,
    space: u64,
    pda_seeds: &[&[u8]],
    system_program: AccountInfoC,
    payer: AccountInfoC,
) -> ProgramResult {
    let rent_sysvar = Rent::get()?;
    let rent_due = rent_sysvar
        .minimum_balance(space as usize)
        .saturating_sub(unsafe { *target_account.lamports });

    // Initialize ix: data
    let mut create_account_ix_data: [u8; 52] = [0; 4 + 8 + 8 + 32];
    let (_disc_bytes, lamport_bytes, space_bytes, owner_bytes) =
        mut_array_refs![&mut create_account_ix_data, 4, 8, 8, 32];
    // Enum discriminator is 0 so we don't need to write anything
    // *_disc_bytes = [0, 0, 0, 0];
    // Write rent cost in lamports as u64 le bytes
    *lamport_bytes = rent_due.to_le_bytes();
    // Write space in bytes as u64 le bytes
    *space_bytes = space.to_le_bytes();
    // Write owner pubkey bytes
    *owner_bytes = owner.to_bytes();

    // Instruction accounts: from, to
    let instruction_accounts = [payer.to_meta_c(), target_account.to_meta_c_signer()];

    // Build instruction
    let create_account_instruction = InstructionC {
        data: create_account_ix_data.as_ptr(),
        data_len: 52,
        accounts: instruction_accounts.as_ptr(),
        accounts_len: 2,
        program_id: &solana_program::system_program::ID,
    };
    let create_account_account_infos = [payer, target_account, system_program];

    let cpi_seeds = &[pda_seeds];
    #[cfg(target_os = "solana")]
    unsafe {
        solana_program::syscalls::sol_invoke_signed_c(
            (&create_account_instruction) as *const InstructionC as *const u8,
            create_account_account_infos.as_ptr() as *const u8,
            3,
            cpi_seeds.as_ptr() as *const u8,
            1,
        );
    }
    #[cfg(not(target_os = "solana"))]
    core::hint::black_box((
        &create_account_instruction,
        &create_account_account_infos,
        cpi_seeds,
    ));

    Ok(())
}

#[allow(unused)]
pub fn check_pda_address(
    seeds: &[&[u8]],
    program_id: &Pubkey,
    actual_key: &Pubkey,
) -> Result<u8, ProgramError> {
    let (key, bump) = {
        #[cfg(target_os = "solana")]
        {
            let mut bytes = [0; 32];
            let mut bump_seed = u8::MAX;
            let result = unsafe {
                solana_program::syscalls::sol_try_find_program_address(
                    seeds as *const _ as *const u8,
                    seeds.len() as u64,
                    program_id as *const _ as *const u8,
                    &mut bytes as *mut _ as *mut u8,
                    &mut bump_seed as *mut _ as *mut u8,
                )
            };
            match result {
                solana_program::entrypoint::SUCCESS => (Pubkey::from(bytes), bump_seed),
                _ => panic!("failed to find seeds for program"),
            }
        }
        #[cfg(not(target_os = "solana"))]
        {
            Pubkey::find_program_address(seeds, program_id)
        }
    };
    if key.eq(actual_key) {
        Ok(bump)
    } else {
        log::sol_log("pda does not match");
        Err(ProgramError::InvalidInstructionData)
    }
}

/// Taken from nightly rust
#[inline(always)]
pub const unsafe fn split_at_unchecked<T>(slice: &[T], mid: usize) -> (&[T], &[T]) {
    // HACK: the const function `from_raw_parts` is used to make this
    // function const; previously the implementation used
    // `(slice.get_unchecked(..mid), slice.get_unchecked(mid..))`

    let len = slice.len();
    let ptr = slice.as_ptr();

    // SAFETY: Caller has to check that `0 <= mid <= slice.len()`
    unsafe {
        (
            core::slice::from_raw_parts(ptr, mid),
            core::slice::from_raw_parts(ptr.add(mid), len - mid),
        )
    }
}

/// Taken from nightly rust
#[inline(always)]
pub unsafe fn split_at_mut_unchecked<T>(slice: &mut [T], mid: usize) -> (&mut [T], &mut [T]) {
    // HACK: the const function `from_raw_parts` is used to make this
    // function const; previously the implementation used
    // `(slice.get_unchecked(..mid), slice.get_unchecked(mid..))`

    let len = slice.len();
    let ptr = slice.as_mut_ptr();

    // SAFETY: Caller has to check that `0 <= mid <= slice.len()`
    unsafe {
        (
            core::slice::from_raw_parts_mut(ptr, mid),
            core::slice::from_raw_parts_mut(ptr.add(mid), len - mid),
        )
    }
}
