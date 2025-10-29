# Trap Death Analysis - Initial Findings

## Summary

Analyzed 6 games from `mixed_luke_craig` dataset where the bot died from being trapped (100% trapped death rate).

## Tool Created

**`analyze_trap_decisions.rs`** - Works backwards from death turn to identify alternative moves that would have survived longer.

## Key Findings

### Game 01 (63 turns, health 97 at death)

**Critical Bad Decisions Identified:**

- **Turn 38**: Chose Down (appears to lead to trap)
  - Position: (9, 5)
  - Health: 85
  - Alternatives Up, Left, Right would have survived 24 more turns

- **Turn 44**: Chose Right (appears to lead to trap)
  - Position: (7, 9)
  - Health: 79
  - Alternatives Up, Down, Left would have survived 18 more turns

- **Turn 50**: Chose Down (appears to lead to trap)
  - Position: (4, 6)
  - Health: 73
  - Alternatives Up, Left would have survived 12 more turns

- **Turn 51**: Chose Down (appears to lead to trap)
  - Position: (4, 7)
  - Health: 72
  - Alternatives Up, Left would have survived 11 more turns

**Pattern**: In the 10 turns before death (turns 52-62), all moves were essentially equivalent - the snake was already doomed. The critical mistakes happened 20-30 turns before actual death.

### Game 03 (93 turns, health 68 at death)

**Similar pattern of bad decisions:**

- **Turn 67**: Chose Down (appears to lead to trap)
  - Alternatives would have survived 19 more turns

- **Turn 69**: Chose Down (appears to lead to trap)
  - Alternatives would have survived 17 more turns

- **Turn 70**: Chose Right (appears to lead to trap)
  - Alternatives would have survived 16 more turns

- **Turn 72**: Chose Left (appears to lead to trap)
  - Alternatives would have survived 14 more turns

## Important Caveats

The current analysis uses **simple legality checking** that:
- Checks bounds
- Checks collision with neck
- Checks collision with snake bodies (excluding tails)

**BUT** it does NOT account for:
- Opponent moves happening simultaneously
- Food spawning
- Complex multi-turn consequences

The moves marked as "led to trap/collision" may have been legal at the time but led to increasingly constrained positions.

## What We've Learned

1. **Traps develop gradually**: The snake isn't making one catastrophically bad move - it's making a series of moves that progressively constrain its space

2. **Early decisions matter**: The critical mistakes happen 15-30 turns before actual death, not in the final moments

3. **All alternatives aren't equal**: At each bad decision point, there were 2-3 alternatives that would have survived significantly longer

## Next Steps to Understand WHY

To understand why the algorithm chose these problematic moves, we need to:

1. **Integrate actual evaluation scores**: For each problematic turn, calculate what score each move got
   - Space control score
   - Health/food score
   - Territory control score
   - Attack score
   - Wall proximity penalty
   - Entrapment penalty

2. **Compare with alternatives**: See which scoring component caused the algorithm to prefer the bad move

3. **Visualize the board**: Create visual representations of these critical turns to understand spatial patterns

4. **Check entrapment detection**: Verify if the new entrapment heuristics are triggering correctly

## Specific Investigation Needed

### Turn 38, Game 01
- Position: (9, 5), Health: 85
- Chose: Down (bad)
- Better alternatives: Up, Left, Right
- **Question**: Why did Down score higher than the alternatives?

### Turn 44, Game 01
- Position: (7, 9), Health: 79
- Chose: Right (bad)
- Better alternatives: Up, Down, Left
- **Question**: Was there food to the right? Did it prioritize food over space?

## User's Observation

User mentioned: "I know there's a case where given the choice between going towards another snake's body and food, it went towards the snake and got trapped, even though the space below the snake was much more wide open."

This suggests:
- Food attraction is overriding space control
- The algorithm may be undervaluing long-term spatial safety
- Need to check if entrapment penalties are strong enough to counteract food attraction

## Recommended Tool Enhancement

Enhance `analyze_trap_decisions.rs` to:
1. Load the bot's config
2. For each bad decision, call the actual evaluation function
3. Show breakdown of scores for each move alternative:
   ```
   Turn 38: Chose Down

   Down:  score=-1523 (space=-500, health=+800, control=-123, ...)
   Up:    score=-234  (space=+200, health=-50, control=+100, ...)
   Left:  score=-156  (space=+300, health=-100, control=+80, ...)
   Right: score=-289  (space=+250, health=-75, control=+90, ...)
   ```

This would immediately reveal which scoring component is causing bad decisions.
