use crate::solana_nostd_entrypoint::{AccountInfoC, InstructionC};
use solana_program::{
    entrypoint::ProgramResult, log, program_error::ProgramError,
    pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};

pub mod spl_token_utils;

/// Creates a new pda.
///
/// # SAFETY:
/// Reads lamports from `target_account`. Som no one must hold
/// a mutable reference to its lamports elsewhere.
#[inline(always)]
pub unsafe fn create_pda_funded_by_payer(
    mut target_account: AccountInfoC,
    owner: &Pubkey,
    space: u64,
    pda_seeds: &[&[u8]],
    system_program: AccountInfoC,
    payer: AccountInfoC,
) -> ProgramResult {
    let rent_sysvar = Rent::get()?;
    let target_account_lamports = unsafe { *target_account.lamports };
    if target_account_lamports == 0 {
        let rent_due = rent_sysvar
            .minimum_balance(space as usize)
            .saturating_sub(unsafe { *target_account.lamports });

        // Initialize ix: data
        let mut create_account_ix_data: [u8; 52] = [0; 4 + 8 + 8 + 32];
        // Enum discriminator is 0 so we don't need to write anything to first 4 bytes
        unsafe {
            // Write rent cost in lamports as u64 le bytes
            core::ptr::copy_nonoverlapping(
                &rent_due as *const u64 as *const u8,
                create_account_ix_data
                    .as_mut_ptr()
                    .add(4),
                8,
            );

            // Write space in bytes as u64 le bytes
            core::ptr::copy_nonoverlapping(
                &space as *const u64 as *const u8,
                create_account_ix_data
                    .as_mut_ptr()
                    .add(12),
                8,
            );

            // Write owner pubkey bytes
            core::ptr::copy_nonoverlapping(
                owner.as_ref().as_ptr(),
                create_account_ix_data
                    .as_mut_ptr()
                    .add(20),
                32,
            );
        }

        // Instruction accounts: from, to
        let instruction_accounts =
            [payer.to_meta_c(), target_account.to_meta_c_signer()];

        // Build instruction
        let create_account_instruction = InstructionC {
            data: create_account_ix_data.as_ptr(),
            data_len: 52,
            accounts: instruction_accounts.as_ptr(),
            accounts_len: 2,
            program_id: &solana_program::system_program::ID,
        };
        let create_account_account_infos =
            [payer, target_account, system_program];

        let cpi_seeds = &[pda_seeds];
        #[cfg(target_os = "solana")]
        unsafe {
            solana_program::syscalls::sol_invoke_signed_c(
                (&create_account_instruction) as *const InstructionC
                    as *const u8,
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
    } else {
        // Can't use create_account on accounts with nonzero lamports.
        //
        // Thus, we need to
        // 1) transfer sufficient lamports for rent exemption -- paid for by the user
        // 2) system_instruction::allocate enough space for the account
        // 3) assign our program as the owner

        // 1) transfer sufficient lamports for rent exemption
        let rent_exempt_balance = rent_sysvar
            .minimum_balance(space as usize)
            .saturating_sub(target_account_lamports);
        if rent_exempt_balance > 0 {
            // Only call transfer instruction if required
            // 12 bytes = [4 byte enum disc][8 byte lamports]
            let mut transfer_ix_data = [0; 12];
            // Transfer discriminant is 2_u32 = [2, 0, 0, 0]
            transfer_ix_data[0] = 2;

            // Write rent cost in lamports as u64 le bytes
            core::ptr::copy_nonoverlapping(
                &rent_exempt_balance as *const u64 as *const u8,
                transfer_ix_data.as_mut_ptr().add(4),
                8,
            );

            // Instruction accounts: from, to
            let instruction_accounts =
                [payer.to_meta_c(), target_account.to_meta_c()];

            // Build instruction
            let transfer_instruction = InstructionC {
                data: transfer_ix_data.as_ptr(),
                data_len: 12,
                accounts: instruction_accounts.as_ptr(),
                accounts_len: 2,
                program_id: &solana_program::system_program::ID,
            };
            let transfer_account_infos =
                [payer.clone(), target_account.clone()];
            let cpi_seeds: &[&[&[u8]]] = &[];
            log::sol_log("transfer");
            #[cfg(target_os = "solana")]
            unsafe {
                solana_program::syscalls::sol_invoke_signed_c(
                    (&transfer_instruction) as *const InstructionC as *const u8,
                    transfer_account_infos.as_ptr() as *const u8,
                    2,
                    cpi_seeds.as_ptr() as *const u8,
                    0,
                );
            }
            #[cfg(not(target_os = "solana"))]
            core::hint::black_box((
                &transfer_instruction,
                &transfer_account_infos,
                cpi_seeds,
            ));
        }

        // 2) system_instruction::allocate enough space for the account
        // 12 bytes = [4 byte enum disc][8 byte space u64]
        let mut allocate_ix_data = [0; 12];
        // Allocate discriminant is 8_u32 = [8, 0, 0, 0]
        allocate_ix_data[0] = 8;

        // Write space in bytes as u64 le bytes
        core::ptr::copy_nonoverlapping(
            &space as *const u64 as *const u8,
            allocate_ix_data.as_mut_ptr().add(4),
            8,
        );

        // Instruction accounts: from, to
        let instruction_accounts = [target_account.to_meta_c_signer()];

        // Build instruction
        let allocate_instruction = InstructionC {
            data: allocate_ix_data.as_ptr(),
            data_len: 12,
            accounts: instruction_accounts.as_ptr(),
            accounts_len: 1,
            program_id: &solana_program::system_program::ID,
        };
        let allocate_account_infos = [target_account.clone()];
        let cpi_seeds: &[&[&[u8]]] = &[pda_seeds];
        log::sol_log("alloc");
        #[cfg(target_os = "solana")]
        unsafe {
            solana_program::syscalls::sol_invoke_signed_c(
                (&allocate_instruction) as *const InstructionC as *const u8,
                allocate_account_infos.as_ptr() as *const u8,
                1,
                cpi_seeds.as_ptr() as *const u8,
                1,
            );
        }
        target_account.data_len = space;
        #[cfg(not(target_os = "solana"))]
        core::hint::black_box((
            &allocate_instruction,
            &allocate_account_infos,
            cpi_seeds,
        ));

        // 3) assign our program as the owner
        // 36 bytes = [4 byte enum disc][32 byte owner pubkey]
        let mut assign_ix_data = [0; 36];
        // Assign discriminant is 1_u32 = [1, 0, 0, 0]
        assign_ix_data[0] = 1;

        // Write owner pubkey bytes
        core::ptr::copy_nonoverlapping(
            owner.as_ref().as_ptr(),
            assign_ix_data.as_mut_ptr().add(4),
            32,
        );

        // Instruction accounts: from, to
        let instruction_accounts = [target_account.to_meta_c_signer()];

        // Build instruction
        let assign_instruction = InstructionC {
            data: assign_ix_data.as_ptr(),
            data_len: 36,
            accounts: instruction_accounts.as_ptr(),
            accounts_len: 1,
            program_id: &solana_program::system_program::ID,
        };
        let assign_account_infos = [target_account];
        let cpi_seeds = &[pda_seeds];
        log::sol_log("assign");
        #[cfg(target_os = "solana")]
        unsafe {
            solana_program::syscalls::sol_invoke_signed_c(
                (&assign_instruction) as *const InstructionC as *const u8,
                assign_account_infos.as_ptr() as *const u8,
                1,
                cpi_seeds.as_ptr() as *const u8,
                1,
            );
        }
        #[cfg(not(target_os = "solana"))]
        core::hint::black_box((
            &assign_instruction,
            &assign_account_infos,
            cpi_seeds,
        ));
    }

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
                solana_program::entrypoint::SUCCESS => {
                    (Pubkey::from(bytes), bump_seed)
                }
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
pub const unsafe fn split_at_unchecked<T>(
    slice: &[T],
    mid: usize,
) -> (&[T], &[T]) {
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
pub unsafe fn split_at_mut_unchecked<T>(
    slice: &mut [T],
    mid: usize,
) -> (&mut [T], &mut [T]) {
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

// #[inline(always)]
// pub(crate) fn pubkey_neq(a: &Pubkey, b: &Pubkey) -> bool {
//     solana_program::program_memory::sol_memcmp(a.as_ref(), b.as_ref(), 32) !=
// 0 }

#[macro_export]
macro_rules! nanolog {
    ($str:literal) => {
        if cfg!(feature = "nanolog") {
            solana_program::log::sol_log($str);
        }
    };
}
