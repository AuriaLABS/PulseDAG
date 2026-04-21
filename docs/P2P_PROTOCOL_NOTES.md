# P2P Protocol Notes

## Topics
- pulsedag.blocks
- pulsedag.txs

## Minimal messages
### Block announcement
- hash
- height
- selected_tip_hint
- parent_hashes

### Full block
- serialized block payload

### Tx announcement
- txid

### Full tx
- serialized tx payload

### Tip response
- selected_tip
- tips[]
- best_height

## Defensive rules
- ignore duplicates early
- reject blocks failing active PoW check
- queue blocks with missing parents as orphans
- cap orphan queue and expire old entries
- journal invalid peer traffic
