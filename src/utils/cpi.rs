extern crate alloc;

use core::ptr::NonNull;

use solana_program::instruction::AccountMeta;
use solana_program::pubkey::Pubkey;

#[repr(C)]
pub(crate) struct StableInstruction {
    pub(crate) accounts: StableView<AccountMeta>,
    pub(crate) data: StableView<u8>,
    pub(crate) program_id: Pubkey,
}

#[repr(C)]
pub(crate) struct StableView<T> {
    ptr: NonNull<T>,
    cap: usize,
    len: usize,
}

impl<T> StableView<T> {
    #[inline(always)]
    pub(crate) fn from_array<const N: usize>(
        array: &mut [T; N],
    ) -> StableView<T> {
        StableView {
            // SAFETY: array implies nonnull
            ptr: unsafe { NonNull::new_unchecked(array.as_mut_ptr()) },
            cap: 0,
            len: N,
        }
    }
}
