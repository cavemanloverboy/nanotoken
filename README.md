
# `nanotoken`



### Notes:
###### Comparisons:
Pubkey comparisons via `PartialEq` cost ≈30 cus. This can be reduced to 10 via memcmp syscall
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

###### `mint_index`
Using a `mint_index` counter has some pros and cons:

*Pros*: 
1. An few extra cus are saved by using a `mint_index: u64` for the mint check instead of doing a pubkey comparison.
2. Creating a token account for a particular mint doesn't require passing in and validating a mint account, as you just need to check if `mint < config.current_mint_index`.

*Cons*
1. Creating a mint account requires a *write* lock on the program config. Here we are making the assumption that mints are not created often enough to care about this (which is true now), so this is ok.
2. Creating a token account for a particular mint requires a *read* lock on the program config.





