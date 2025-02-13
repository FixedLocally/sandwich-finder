# Solana Sandwich Finder
## Overview
Slot range:[318555120, 320319077]
### Global Metrics
|Metric|Value|
|---|---|
|Proportion of sandwich-inclusive block|11.373%|
|Average sandwiches per block|0.16050|
|Standard Deviation of sandwiches per block|0.83227|


### Stake pool dsitribution (Epoch 741):
|Pool|Stake (SOL)|Pool Share|
|---|---|---|
|Marinade (overall)|5,296,455|57.18%|
| - Marinade Liquid|3,084,542|62.09%|
| - Marinade Native|2,211,912|51.50%|
|Jito|4,252,432|27.70%|
|xSHIN|139,469|14.20%|
|JPool|140,220|13.81%|
|SFDP|3,338,093|9.13%|
|BlazeStake|39,230|3.51%|
|The Vault|11,469|0.90%|

### Honourable Mention
These are hand-picked, visible to the naked eye colluders.
|Validator|Stake|Observed Leader Blocks|Weighted Sandwich-inclusive blocks|Weighted Sandwiches|
|---|---|---|---|---|
|P2P.org|6,330,057|23,752|6,942.75|9,127.25|
|StakeHaus - 0% Fee on Rewards/MEV|1,991,967|8,036|1,670|2,229.42|
|AG 0% fee + ALL MEV profit share|1,549,042|6,404|1,743.33|2,279.75|
|Allnodes ⚡️ 0% fee|1,206,244|4,968|1,616.75|2,197.42|
|Private GRt2...LXV8|1,192,153|4,776|1,401|1,846.25|
|HM5H...dMRA|1,143,218|4,336|1,260.75|1,686.25|
|Chorus One|862,361|3,308|988.33|1,285.92|

## Preface
Sandwiching refers to the action of forcing the earlier inclusion of a transaction (frontrun) before a transaction published earlier (victim), with another transaction after the victim transaction to realise a profit (backrun), while abusing the victim's slippage settings. We define a sandwich as "a set of transactions that include exactly one frontrun and exactly one backrun transaction, as well as at least one victim transaction", a sandwicher as "a party that sandwiches", and a colluder as "a validator that forwards transactions they receive to a sandwicher".

Some have [mentioned that](https://discord.com/channels/938287290806042626/938287767446753400/1325923301205344297) users should issue transactions with lower slippage instead but it's not entirely possible when trading token pairs with extremely high volatility. Being forced to issue transactions with low slippage may lead to higher transaction failure rates and missed opportunities, which is also suboptimal.

The reasons why sandwiching is harmful to the ecosystem had been detailed by [another researcher](https://github.com/a-guard/malicious-validators/blob/main/README.md#why-are-sandwich-attacks-harmful) and shall not be repeated in detail here, but it mainly boils down to breaking trust, transparency and fairness.

We believe that colluder identification should be a continuous effort since [generating new keys](https://docs.anza.xyz/cli/wallets/file-system) to run a new validator is essentially free, and with a certain stake pool willing to sell stake to any validator regardless of operating history, one-off removals will prove ineffective. This repository aims to serve as a tool to continuously identify sandwiches and colluders such that relevant parties can remove stake from sandwichers as soon as possible.

## Methodology
### Sandwich identification
A sandwich is defined by a set of transactions that satisfies all of the following:

1. Has at least 3 transactions of strictly increasing inclusion order (frontrun-victims-backrun);
2. The frontrun and the victim transactions trades in the same direction, the backrun's one is in reverse;
3. Output of backrun >= Input of frontrun and Output of frontrun >= Input of backrun (profitability constraint);
4. All transactions use the same AMM;
5. Each victim transaction's signer differs from the frontrun's and the backrun's;
6. A wrapper program is present in the frontrun and backrun and are the same;
   
For each sandwich identified in newly emitted blocks by the cluster, we insert that to a database for report generation.

Note that we don't require the frontrun and the backrun to have the same signer as it's a valid strategy to use multiple wallets to evade detection by moving tokens across wallets.

### Report generation
With the sandwich dataset, we're able to calculate the cluster wide and per validator proportion of sandwich-inclusive blocks and sandwich per block. Our hypothesis is that colluders will exhibit above cluster average values on both metrics. Due to transaction landing delays, the report generation tool also "credits" sandwiches to earlier slots.

The hypothesises are as follows:<br />
Null hypothesis: At least one metric is in line with the cluster average<br />
Alternative hypothesis: Both metrics exceeds cluster average<br />

For the proportion of sandwich-inclusive blocks metric, each block is treated as a Bernoulli trial, where success means a block is sandwich-inclusive and failure means the otherwise. For each validator, the number of blocks emitted (N) and the number of sandwich-inclusive blocks (k) is used to calculate a 99.99% confidence interval of their true proportion of sandwich-inclusion blocks. A validator will be deemed to be above cluster average if the lower bound of the confidence interval is above the cluster average.

For the sandwiches per block metric, the mean and standard deviation of the cluster wide number of sandwiches per block is taken, and a 99.99% confidence interval of the expected number of sandwiches per block should the validator is in line with the cluster wide average is calculated. A validator will be deemed to be above cluster average if the validator's metric is above the confidence interval's upper bound.

Validators satisfying the alternative hypothesis, signaling collusion for an extended period, will be flagged.

For flagging on [Hanabi Staking's dashboard](https://hanabi.so/marinade-stake-selling), flagged validators with fewer than 50 blocks as well as those only exceeding the thresholds marginally but reputable are excluded.