# Code style
- Use OOP-style programming guide in rust where applicable so that object representation is easy to understand and maintain
- Avoid the use of unsafe! code blocks at all costs
- You should strive to use simple representations wherever possible
- Identify whether or not a task is CPU or I/O bound and use appropriate threading library for each
  - For I/O bound or async API response endpoints use `tokio`
  - For CPU bound use `rayon`
- Prefer the use of atomics over locks, but if you must use locks, use `parking_lot` crate over `std`
- Functions should avoid being too complicated, please break things apart into small, easily testable functions
  - Cognitive complexity less than 15 recommended
- Avoid cloning if at all possible
  - Read-only shared data should always be passed by immutable reference or Arc if it needs to cross thread boundaries
- Be mindful of hotspots that are created in memory if doing concurrent tasks
  - Example, rather than a single atomic per thread that can create excessive contention, consider an atomic per thread that a separate thread or the main thread can evaluate and pick overall best result
- Use constants wherever appropriate, and then consider marshalling into a Snake.tomi file for tunable parameters
  - Response time budget ms (400ms by default)
  - Polling interval ms (50ms by default)
  - Weights (see priorities section)

# Workflow
- Be sure to validate static analysis when you're done making a series of code changes
- Validate complication via `cargo` and try to resolve any compiler issues, taking the coding style into account
- You need NOT generate any tests unless explicitly asked to do so

# Test cases
- When asked to create test cases, the cases should be in JSON input format that can be deserialzied per existing data contracts
- Expectations should be clearly defined and understood. If you don't understand or the requirements are vague, please seek additional clarification from the developer before proceeding
- When executing tests, prefer to run the single test or subset of tests being considered rather than the full suite for performance

# Problem statement
You are a competitive programmer for the Battlesnakes game (https://docs.battlesnake.com/). Your task is to collaborate with the developer to produce a working snake bot in Rust using the best of breed solutions and algorithms avaliable to the task. We want to be able to scan the solution space on each turn and use the `MaxN` algorithm to compute our best possible move within the given time limit (see `constraints` and `priorities` section). 

# MaxN Algorithm
The `MaxN` algorithm is a variation of the `MiniMax` or `MinMax` algorithm as seen in engine based games. This algorithm concurrently evaluates the best move that each opposing snake can make, and uses the scoring function for each snake to compute the best move our snake can make (see `priorities` section). This algorithm should be implemented using `rayon` and given a reference to the appropriate data structure that represents each snake's optimal move, go across each snake and determine its best move before the required response time elapses.

Note: In the event that there is only one CPU available for rayon, the algorithm should gracefully degrade to computing the next logical move for each snake in turn in a breadth-first manner, rather than diving too far deep for a single snake on the board. Conversely, if there's only a single snake on the board, and there are multiple CPUs available, the system should fan out across each possible direction and determine the snake's optimum move set within the given time-limit. In this scenario, it should ONLY overwrite the result of another thread if the weight of the snake's MaxN move is determined to be the best.

There will be a thread that every `POLLING_INTERVAL_MS` checks and computes what our ideal move should be based on the MaxN. We will measure the compute time of this (our budget), and keep track of the average compute time. If we find our timing remaining of `RESPONSE_TIME_BUDGET_MS` will be exceeded before the next poll, we will return the next expected move immediately so as not to time out.

# Constraints
- The `move` endpoint must always return a response in less than `RESPONSE_TIME_BUDGET_MS`

# Priorities

## Our Snake
The following priorities are in order
- Do NOT collide with a wall or another snake in a manner that would cause our snake to die
- Defensively maneauver around other snakes to evade collision
- Obtain food if the possible trap or collision weight is reasonably low
- Determine a trap weight defined as our ability to successfully trap the snake given its most likely move
  - A trap is defined as being able to successfully enlose ourselves around a snake so that they are enclosed by our body and will eventually die
  - If we detect we've entrapped a snake keep the trap maintained for as long as we are not in dire need of food
  - Collect food if and only if the food does not exist within our entrapped state

## Opposing Snakes
The following priorties are in order
- Do NOT collidate with a wall or our snake
- Attempt to entrap a or collidate with our snake in a manner that would cause our snake to die
- Attempt to get food