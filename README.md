# Solana Sandwich Finder
## Overview
Slot range: [318555120, 323474600]
### Global Metrics
|Metric|Value|
|---|---|
|Proportion of sandwich-inclusive block|8.201%|
|Average sandwiches per block|0.11083|
|Standard Deviation of sandwiches per block|0.58824|


### Stake pool dsitribution (Epoch 744):
|Pool|Stake (SOL)|Pool Share|
|---|---|---|
|Marinade (overall)|6,125,919|64.97%|
| - Marinade Liquid|3,233,492|64.64%|
| - Marinade Native|2,892,427|65.36%|
|xSHIN|327,505|33.20%|
|Jito|4,755,413|31.94%|
|SFDP|5,638,972|13.94%|
|JPool|136,538|13.88%|
|BlazeStake|38,361|4.12%|
|The Vault|11,494|0.81%|

### Honourable Mention
These are hand-picked, visible to the naked eye colluders.
|Validator|Stake|Observed Leader Blocks|Weighted Sandwich-inclusive blocks|Weighted Sandwiches|
|---|---|---|---|---|
|P2P.org|5,056,770|76,544|10,495.17|13,608.17|
|StakeHaus - 0% Fee on Rewards/MEV|1,972,082|25,228|3,560.83|4,608.42|
|AG 0% fee + ALL MEV profit share|1,537,206|19,416|3,720.08|4,700.75|
|Private GRt2...LXV8|1,192,153|15,532|2,276.75|2,941.17|
|Allnodes ⚡️ 0% fee|1,187,931|15,764|2,242.42|3,014.92|
|RA|1,148,704|14,608|2,846.83|3,607.17|
|Chorus One|992,224|11,248|1,567.42|2,009.17|

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

## Report Interpretation
The reports consist of 14 columns and their meanings are as follows:
|Column(s)|Meaning|
|---|---|
|leader/vote|The validator's identity and vote account pubkeys|
|name|The validator's name according to onchain data|
|Sc|"Score", normalised weighted number of sandwiches|
|Sc_p|"Presence score", normalised number of blocks with sandwiches, which roughly means proportion of sandwich inclusive blocks|
|R-Sc/R-Sc_p|Unnormalised Sc and Sc_p|
|slots|Number of leader slots observed for the validator|
|Sc_p_{lb\|ub}|Bounds of the confidence interval of the validator's true proportion of sandwich inclusive blocks. Flagged if the lower bound is above the cluster mean|
|Sc_{lb\|ub}|Bounds of the confidence interval of which the validator is considered to have an "average" number of sandwiches per block. Flagged if Sc_p is above the upper bound|
{Sc_p\|Sc}_flag|True if the validator is being flagged due to the respective metric, false otherwise|

## Dataset Access
For dataset access, [join the Hanabi Staking Discord](https://discord.gg/VpJuWFRJfb) and open a ticket.