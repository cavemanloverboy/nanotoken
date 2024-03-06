
# `nanotoken`

A zerocopy, `no_alloc` token program for solana that is highly optimized for transfers. The program supports batch invocations, allowing multiple instructions to be executed within a single program invocation. If it were to be used, this could reduce token program blockspace on mainnet from 8-10% to 1-3%.

# Notes/TODOs:
# 1) Comparisons:
Pubkey comparisons via `PartialEq` cost several dozen compute units. This can be reduced to 10 via memop comparison
```rust
fn mem_op_consume(invoke_context: &mut InvokeContext, n: u64) -> Result<(), Error> {
    let compute_budget = invoke_context.get_compute_budget();
    let cost = compute_budget.mem_op_base_cost.max(
        n.checked_div(compute_budget.cpi_bytes_per_unit)
            .unwrap_or(u64::MAX),
    );
    consume_compute_meter(invoke_context, cost)
}
```
As of this writing, `mem_op_base_cost = 10` and `cpi_bytes_per_unit = 250`. So, for `n = 32` this cost evaluates to `10`.

# 2) `mint_index`
Instead of a 32-byte pubkey identifier, an 8-byte `mint_index` counter is used to distinguish between mints. This has some pros and cons:

*Pros*: 
1. An few extra cus are saved by using a `mint_index: u64` for the mint check instead of doing a pubkey comparison, and the mint is 24 bytes smaller.
2. Creating a token account for a particular mint doesn't require passing in and validating a mint account, as you just need to check if `mint < config.current_mint_index`.

*Cons*
1. Creating a mint account requires a *write* lock on the program config. Here we are making the assumption that mints are not created often enough to care about this (which is true now), so this is ok.
2. Creating a token account for a particular mint requires a *read* lock on the program config (as opposed to a read lock on a mint account).

I think this was a mistake... it becomes a little difficult to batch a create mint + create token account since the mint index is not known in advance. The mint identifier should either be switched back to a pubkey, or the batched invocations should be made stateful so that a create account request with a null index (e.g. `u64::MAX`) uses the most recently created mint index in the invocation.

# 3) init if needed
Presently, a nanotoken token account is initialized if needed during a transmute operation when going from tokenkeg --> nanotoken. However, this is not done on the return trip. It would be a better user experience if it was also done on the return trip.

# 4) Non-canonical token accounts
Presently, a user can only have one token account (and token accounts cannot have authority transferred). At the request of two developers I deeply respect and admire, non-canonical token accounts should be permitted.

# 5) Hammer
The hammer cli is absolutely embarrassing spaghetti and inefficient. I am ashamed. Don't shame me further for it. My rustfmt didn't even work on the main file lol...

# 6) zero-dependency sdk
It should be possible to write a zero-dependency sdk for this program. I will get to that at some point
