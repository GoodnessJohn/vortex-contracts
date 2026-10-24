/**
 * @vortex-protocol/types
 *
 * TypeScript type definitions for Vortex Protocol smart contracts.
 * Automatically generated from Rust struct/enum definitions in
 * intent_settlement/src/lib.rs to guarantee type parity.
 *
 * Issue #138
 */

/**
 * Intent state enumeration
 * Represents the lifecycle state of a cross-chain swap intent
 */
export enum IntentState {
  Open = "Open",                  // Awaiting solver assignment
  Accepted = "Accepted",          // Solver has claimed fill rights
  PartiallyFilled = "PartiallyFilled", // One or more partial fills delivered
  Filled = "Filled",              // User received >= min_dst_amount
  Cancelled = "Cancelled",        // User cancelled before fill
  Expired = "Expired",            // Deadline passed without fill
  Slashed = "Slashed",            // Solver failed to fill after accepting
  Bidding = "Bidding",            // Reserved for future competitive bidding
  Filling = "Filling",            // Solver has called begin_fill; escrow held
  Disputed = "Disputed",          // User contested the fill during dispute window
  Resolved = "Resolved",          // Arbiter closed the dispute
}

/**
 * Arbiter's ruling on a disputed fill (Issue #188)
 */
export enum DisputeResolution {
  Upheld = "Upheld",      // Arbiter sided with user; solver slashed
  Dismissed = "Dismissed", // Arbiter sided with solver; no slash applied
}

/**
 * A user's signed request to swap tokens cross-chain
 */
export interface IntentRecord {
  /** Unique identifier for this intent (hash of user + timestamp + nonce) */
  intent_id: string; // BytesN<32> as hex string

  /** User initiating the swap */
  user: string; // Stellar Address

  /** Source chain (e.g. "ethereum", "base", "polygon") */
  src_chain: string;

  /** Token address on source chain */
  src_token: string;

  /** Amount to swap in source token's smallest unit */
  src_amount: string; // i128 as string for precision

  /** Destination token on Stellar (SAC/SEP-41 contract address) */
  dst_token: string; // Stellar Address

  /** Minimum acceptable output per fill */
  min_dst_amount: string; // i128 as string for precision

  /** Assigned solver (if any) */
  solver?: string; // Stellar Address | null

  /** Current lifecycle state */
  state: IntentState;

  /** Timestamp when intent was created */
  created_at: number; // u64

  /** Deadline by which intent must be filled or cancelled */
  deadline: number; // u64

  /** Timestamp when intent was first filled (if filled) */
  filled_at?: number; // u64 | null

  /** Cumulative dst tokens received across all fills */
  fill_amount?: string; // i128 | null (as string for precision)

  /** Bond token backing this intent's fill guarantee */
  bond_token: string; // Stellar Address

  /** End of the escrow/dispute window (Issue #188) */
  dispute_deadline?: number; // u64 | null

  /** Timestamp when dispute was raised (Issue #188) */
  dispute_raised_at?: number; // u64 | null

  /** Arbiter's decision on the dispute (Issue #188) */
  resolution?: DisputeResolution; // null if no dispute

  /** Cumulative dst tokens delivered so far */
  total_filled: string; // i128 as string for precision

  /** Solver registry tier at time of accept (Issue #197) */
  solver_tier: number; // u32
}

/**
 * A registered solver (off-chain market maker)
 */
export interface SolverRecord {
  /** Solver's Stellar address */
  address: string;

  /** Bond amount in the default bond token (USDC by default) */
  bond_amount: string; // i128 as string for precision

  /** Number of intents successfully filled */
  fills_completed: number; // u32

  /** Number of intents failed (missed fill window) */
  fills_failed: number; // u32

  /** Cumulative volume of dst tokens delivered */
  total_volume: string; // i128 as string for precision

  /** Whether the solver is active (can accept new intents) */
  is_active: boolean;

  /** Timestamp when solver registered */
  registered_at: number; // u64

  /** Number of intents currently Accepted (not yet filled/slashed) */
  active_intents: number; // u32

  /** Timestamp of last slash (used for cooldown enforcement) */
  last_slash_time: number; // u64

  /** Approved bond tokens this solver holds bonds in (Issue #187) */
  bond_tokens: string[]; // Vec<Stellar Address>
}

/**
 * Reputation snapshot preserved across deregister/re-register cycles (Issue #272)
 */
export interface ReputationSnapshot {
  /** Timestamp of most recent slash (for cooldown enforcement) */
  last_slash_time: number; // u64

  /** Cumulative successful fills (preserved) */
  fills_completed: number; // u32

  /** Cumulative failed fills (preserved) */
  fills_failed: number; // u32

  /** Cumulative volume delivered (preserved) */
  total_volume: string; // i128 as string for precision
}

/**
 * Protocol error codes
 * Match the Rust Error enum discriminants exactly
 */
export enum ErrorCode {
  AlreadyInitialized = 1,
  Unauthorized = 2,
  IntentNotFound = 3,
  IntentNotOpen = 4,
  IntentExpired = 5,
  IntentNotAccepted = 6,
  SolverNotRegistered = 7,
  SolverBondTooLow = 8,
  InsufficientOutput = 9,
  FillWindowExpired = 10,
  CannotCancelAccepted = 11,
  SolverInactive = 12,
  ZeroAmount = 13,
  InvalidDeadline = 14,
  IntentAlreadyFilled = 15,
  NotInitialized = 16,
  SolverHasActiveIntents = 17,
  ContractPaused = 18,
  DeadlineNotReached = 19,
  InsufficientBond = 20,
  DstTokenNotAllowed = 21,
  IntentAlreadyExists = 22,
  FeeOverflow = 23,
  InvalidTokenInterface = 24,
  NoPendingFeeRecipient = 25,
  SrcChainNotAllowed = 26,
  RescueProtectedToken = 27,
  InvalidSrcToken = 28,
  ProofRegistryNotSet = 30,
  ProofNotFound = 31,
  ProofChainMismatch = 32,
  ProofPayloadMismatch = 33,
  SlasherNotAllowed = 34,
  ExtensionLimitReached = 35,
  DeregisterBeforeReregister = 36,
  InvalidConfig = 37,
  InvalidFeeTiers = 38,
  NoPendingAdmin = 39,
  TimelockNotReady = 40,
  NoUpgradePending = 41,
  UpgradeAlreadyApplied = 42,
  MinBondMultiplierNotFound = 43,
  AdminNotInAllowlist = 44,
  FeeDiscountNotFound = 45,
}

/**
 * Error message map for error code documentation
 */
export const ErrorMessages: Record<ErrorCode, string> = {
  [ErrorCode.AlreadyInitialized]: "Contract is already initialized",
  [ErrorCode.Unauthorized]: "Caller is not authorized for this operation",
  [ErrorCode.IntentNotFound]: "Intent not found in storage",
  [ErrorCode.IntentNotOpen]: "Intent is not in Open state",
  [ErrorCode.IntentExpired]: "Intent has expired",
  [ErrorCode.IntentNotAccepted]: "Intent is not in Accepted state",
  [ErrorCode.SolverNotRegistered]: "Solver is not registered",
  [ErrorCode.SolverBondTooLow]: "Solver bond is below minimum",
  [ErrorCode.InsufficientOutput]: "Fill amount is below minimum dst_amount",
  [ErrorCode.FillWindowExpired]: "Fill window has expired",
  [ErrorCode.CannotCancelAccepted]: "Cannot cancel an Accepted intent",
  [ErrorCode.SolverInactive]: "Solver is inactive",
  [ErrorCode.ZeroAmount]: "Amount must be positive",
  [ErrorCode.InvalidDeadline]: "Deadline is in the past",
  [ErrorCode.IntentAlreadyFilled]: "Intent is already filled",
  [ErrorCode.NotInitialized]: "Contract is not initialized",
  [ErrorCode.SolverHasActiveIntents]: "Solver has active intents",
  [ErrorCode.ContractPaused]: "Contract is paused",
  [ErrorCode.DeadlineNotReached]: "Deadline has not been reached",
  [ErrorCode.InsufficientBond]: "Insufficient bond balance",
  [ErrorCode.DstTokenNotAllowed]: "Destination token is not allowed",
  [ErrorCode.IntentAlreadyExists]: "Intent already exists",
  [ErrorCode.FeeOverflow]: "Fee arithmetic overflow",
  [ErrorCode.InvalidTokenInterface]: "Token does not implement SEP-41",
  [ErrorCode.NoPendingFeeRecipient]: "No pending fee recipient proposal",
  [ErrorCode.SrcChainNotAllowed]: "Source chain is not allowed",
  [ErrorCode.RescueProtectedToken]: "Cannot rescue the bond token",
  [ErrorCode.InvalidSrcToken]: "Invalid source token format",
  [ErrorCode.ProofRegistryNotSet]: "Proof registry is not set",
  [ErrorCode.ProofNotFound]: "Proof not found",
  [ErrorCode.ProofChainMismatch]: "Proof chain mismatch",
  [ErrorCode.ProofPayloadMismatch]: "Proof payload mismatch",
  [ErrorCode.SlasherNotAllowed]: "Slasher is not allowed",
  [ErrorCode.ExtensionLimitReached]: "Extension limit reached",
  [ErrorCode.DeregisterBeforeReregister]: "Must deregister before re-registering",
  [ErrorCode.InvalidConfig]: "Invalid configuration",
  [ErrorCode.InvalidFeeTiers]: "Invalid fee tier configuration",
  [ErrorCode.NoPendingAdmin]: "No pending admin proposal",
  [ErrorCode.TimelockNotReady]: "Timelock period has not elapsed",
  [ErrorCode.NoUpgradePending]: "No upgrade pending",
  [ErrorCode.UpgradeAlreadyApplied]: "Upgrade already applied",
  [ErrorCode.MinBondMultiplierNotFound]: "Min bond multiplier not found",
  [ErrorCode.AdminNotInAllowlist]: "Admin not in allowlist",
  [ErrorCode.FeeDiscountNotFound]: "Fee discount not found",
};

/**
 * Get a human-readable error message
 */
export function getErrorMessage(code: ErrorCode | number): string {
  return ErrorMessages[code as ErrorCode] || `Unknown error: ${code}`;
}

/**
 * Version of these types (should match intent_settlement version)
 */
export const TYPES_VERSION = "0.1.0";

/* =========================================================================
 * governor
 * ========================================================================= */

/**
 * A single voting checkpoint for an account (governor::Checkpoint)
 */
export interface Checkpoint {
  /** Ledger sequence at which the checkpoint was recorded */
  ledger: number; // u32

  /** Cumulative voting power at this checkpoint */
  votes: string; // i128 as string for precision
}

/**
 * Vote direction (governor::VoteType)
 */
export enum VoteType {
  Against = "Against",
  For = "For",
  Abstain = "Abstain",
}

/**
 * Lifecycle state of a proposal (governor::ProposalState)
 */
export enum ProposalState {
  Pending = "Pending",
  Active = "Active",
  Defeated = "Defeated",
  Succeeded = "Succeeded",
  Queued = "Queued",
  Executed = "Executed",
  Cancelled = "Cancelled",
}

/**
 * A governance proposal (governor::Proposal)
 */
export interface Proposal {
  /** Unique proposal identifier */
  id: number; // u64

  /** Account that created the proposal */
  proposer: string; // Stellar Address

  /** Ordered list of contract calls to execute on success */
  targets: string[]; // Vec<Stellar Address>

  /** Function names to invoke on each target */
  functions: string[]; // Vec<String>

  /** Arguments for each call */
  args: string[]; // Vec<String>

  /** Ledger at which voting begins */
  start_ledger: number; // u32

  /** Ledger at which voting ends */
  end_ledger: number; // u32

  /** Votes cast in favour */
  for_votes: string; // i128 as string for precision

  /** Votes cast against */
  against_votes: string; // i128 as string for precision

  /** Votes abstained */
  abstain_votes: string; // i128 as string for precision

  /** Whether the proposal has been executed */
  executed: boolean;

  /** Whether the proposal has been cancelled */
  cancelled: boolean;
}

/**
 * Governor configuration (governor::GovernorConfig)
 */
export interface GovernorConfig {
  /** Governance token contract address */
  token: string; // Stellar Address

  /** Voting delay in ledgers after proposal creation */
  voting_delay: number; // u32

  /** Voting period in ledgers */
  voting_period: number; // u32

  /** Minimum votes required for a proposal to succeed */
  quorum: string; // i128 as string for precision

  /** Minimum votes required to submit a proposal */
  proposal_threshold: string; // i128 as string for precision

  /** Timelock delay in ledgers before execution */
  timelock_delay: number; // u32
}

/**
 * Governor error codes (governor::Error)
 */
export enum GovernorError {
  AlreadyInitialized = 1,
  NotInitialized = 2,
  Unauthorized = 3,
  ProposalNotFound = 4,
  ProposalNotActive = 5,
  ProposalNotSucceeded = 6,
  ProposalNotQueued = 7,
  ProposalAlreadyExecuted = 8,
  ProposalAlreadyCancelled = 9,
  VotingClosed = 10,
  VotingNotStarted = 11,
  AlreadyVoted = 12,
  BelowProposalThreshold = 13,
  QuorumNotReached = 14,
  TimelockNotReady = 15,
  InvalidConfig = 16,
  ZeroAmount = 17,
  NoVotingPower = 18,
}

/* =========================================================================
 * treasury_streams
 * ========================================================================= */

/**
 * A linear vesting stream (treasury_streams::Stream)
 */
export interface Stream {
  /** Unique stream identifier */
  id: number; // u64

  /** Account that created the stream (funder) */
  sender: string; // Stellar Address

  /** Account receiving the streamed tokens */
  recipient: string; // Stellar Address

  /** Streamed token contract address */
  token: string; // Stellar Address

  /** Total amount to be streamed over the duration */
  total_amount: string; // i128 as string for precision

  /** Amount already withdrawn by the recipient */
  withdrawn: string; // i128 as string for precision

  /** Ledger timestamp at which streaming begins */
  start_time: number; // u64

  /** Ledger timestamp at which streaming ends */
  end_time: number; // u64

  /** Whether the stream has been cancelled */
  cancelled: boolean;

  /** Whether the stream has been fully withdrawn/closed */
  closed: boolean;
}

/**
 * Storage keys for treasury_streams (treasury_streams::DataKey)
 */
export enum TreasuryStreamsDataKey {
  Admin = "Admin",
  NextStreamId = "NextStreamId",
  Stream = "Stream",
}

/**
 * Treasury streams error codes (treasury_streams::Error)
 */
export enum TreasuryStreamsError {
  AlreadyInitialized = 1,
  NotInitialized = 2,
  Unauthorized = 3,
  StreamNotFound = 4,
  InvalidTimeRange = 5,
  ZeroAmount = 6,
  StreamCancelled = 7,
  StreamClosed = 8,
  NothingToWithdraw = 9,
  InsufficientBalance = 10,
  InvalidConfig = 11,
}

/* =========================================================================
 * fee_router
 * ========================================================================= */

/**
 * A fee sink destination (fee_router::Sink)
 */
export interface Sink {
  /** Recipient address for routed fees */
  recipient: string; // Stellar Address

  /** Share of fees routed to this sink, in basis points */
  share_bps: number; // u32
}

/**
 * Fee router configuration (fee_router::RouterConfig)
 */
export interface FeeRouterConfig {
  /** Token in which fees are denominated */
  fee_token: string; // Stellar Address

  /** Ordered list of fee sinks */
  sinks: Sink[]; // Vec<Sink>

  /** Whether the router is paused */
  paused: boolean;
}

/**
 * Fee router error codes (fee_router::Error)
 */
export enum FeeRouterError {
  AlreadyInitialized = 1,
  NotInitialized = 2,
  Unauthorized = 3,
  InvalidSinks = 4,
  SinkNotFound = 5,
  ZeroAmount = 6,
  Overflow = 7,
  ContractPaused = 8,
  InvalidConfig = 9,
  NoSinks = 10,
}

/* =========================================================================
 * backstop_vault
 * ========================================================================= */

/**
 * A depositor's position in the backstop vault (backstop_vault::Position)
 */
export interface BackstopPosition {
  /** Account that owns the position */
  owner: string; // Stellar Address

  /** Amount of tokens deposited */
  amount: string; // i128 as string for precision

  /** Ledger timestamp at which the position was opened */
  deposited_at: number; // u64

  /** Ledger timestamp at which the lockup expires */
  unlock_at: number; // u64

  /** Whether the position has been withdrawn */
  withdrawn: boolean;
}

/**
 * Vault configuration (backstop_vault::VaultConfig)
 */
export interface BackstopVaultConfig {
  /** Token accepted for deposits */
  token: string; // Stellar Address

  /** Minimum deposit amount */
  min_deposit: string; // i128 as string for precision

  /** Lockup duration in seconds */
  lockup_period: number; // u64

  /** Maximum total deposits the vault will accept */
  capacity: string; // i128 as string for precision
}

/**
 * Backstop vault error codes (backstop_vault::Error)
 */
export enum BackstopVaultError {
  AlreadyInitialized = 1,
  NotInitialized = 2,
  Unauthorized = 3,
  PositionNotFound = 4,
  BelowMinDeposit = 5,
  CapacityExceeded = 6,
  StillLocked = 7,
  AlreadyWithdrawn = 8,
  ZeroAmount = 9,
  InvalidConfig = 10,
  ContractPaused = 11,
}

/* =========================================================================
 * intent_periphery
 * ========================================================================= */

/**
 * A batched intent submission (intent_periphery::BatchIntent)
 */
export interface BatchIntent {
  /** Source chain identifier */
  src_chain: string;

  /** Token address on source chain */
  src_token: string;

  /** Amount to swap in source token's smallest unit */
  src_amount: string; // i128 as string for precision

  /** Destination token on Stellar */
  dst_token: string; // Stellar Address

  /** Minimum acceptable output */
  min_dst_amount: string; // i128 as string for precision

  /** Deadline by which the intent must be filled */
  deadline: number; // u64
}

/**
 * Result of a batch submission (intent_periphery::BatchResult)
 */
export interface BatchResult {
  /** Intent IDs created for each submitted intent */
  intent_ids: string[]; // Vec<BytesN<32> as hex string>

  /** Number of intents successfully created */
  created: number; // u32

  /** Number of intents that failed validation */
  failed: number; // u32
}

/**
 * Intent periphery error codes (intent_periphery::Error)
 */
export enum IntentPeripheryError {
  AlreadyInitialized = 1,
  NotInitialized = 2,
  Unauthorized = 3,
  EmptyBatch = 4,
  BatchTooLarge = 5,
  InvalidIntent = 6,
  ZeroAmount = 7,
  InvalidDeadline = 8,
  SettlementCallFailed = 9,
  InvalidConfig = 10,
}
</｜｜DSML｜｜ parameter>
</｜｜DSML｜｜ invoke>
</｜｜DSML｜｜ calls>