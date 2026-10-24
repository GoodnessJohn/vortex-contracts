#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env, Vec,
};

/// Storage keys for the treasury streams contract.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Address allowed to create/cancel streams (timelock or governor).
    Governor,
    /// Monotonic counter for stream ids.
    NextId,
    /// Per-stream record.
    Stream(u64),
    /// Total number of streams ever created.
    StreamCount,
    /// Ids of streams created by a given sender.
    SenderStreams(Address),
    /// Ids of streams created for a given recipient.
    RecipientStreams(Address),
    /// Circuit-breaker flag gating `create_stream` and `withdraw`.
    Paused,
}

/// A single payment stream.
#[contracttype]
#[derive(Clone)]
pub struct Stream {
    pub sender: Address,
    pub recipient: Address,
    pub token: Address,
    pub total: i128,
    pub start: u64,
    pub cliff: u64,
    pub end: u64,
    pub withdrawn: i128,
    pub cancelled: bool,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    StreamNotFound = 4,
    InvalidSchedule = 5,
    InvalidAmount = 6,
    NothingToWithdraw = 7,
    AlreadyCancelled = 8,
    ArithmeticOverflow = 9,
    Paused = 10,
    NotPaused = 11,
}

#[contract]
pub struct TreasuryStreams;

#[contractimpl]
impl TreasuryStreams {
    /// One-time initialization setting the governor (timelock/governor) address.
    pub fn initialize(env: Env, governor: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Governor) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Governor, &governor);
        env.storage().instance().set(&DataKey::NextId, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Pauses `create_stream` and `withdraw`. Only the governor may call this.
    ///
    /// `cancel` deliberately remains available while paused so that unvested
    /// funds can still be returned to senders during an incident.
    pub fn pause(env: Env) -> Result<(), Error> {
        Self::require_governor(&env)?;
        if Self::is_paused(&env) {
            return Err(Error::Paused);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events()
            .publish((soroban_sdk::symbol_short!("pause"),), ());
        Ok(())
    }

    /// Lifts the pause. Only the governor may call this.
    pub fn unpause(env: Env) -> Result<(), Error> {
        Self::require_governor(&env)?;
        if !Self::is_paused(&env) {
            return Err(Error::NotPaused);
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((soroban_sdk::symbol_short!("unpause"),), ());
        Ok(())
    }

    /// Returns whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Creates a new stream funded by `sender`. Only the governor may call this.
    pub fn create_stream(
        env: Env,
        sender: Address,
        recipient: Address,
        token: Address,
        total: i128,
        start: u64,
        cliff: u64,
        end: u64,
    ) -> Result<u64, Error> {
        let governor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Governor)
            .ok_or(Error::NotInitialized)?;
        governor.require_auth();

        if Self::is_paused(&env) {
            return Err(Error::Paused);
        }

        if total <= 0 {
            return Err(Error::InvalidAmount);
        }
        if end <= start || cliff < start || cliff > end {
            return Err(Error::InvalidSchedule);
        }

        // Escrow the full deposit into the contract up front.
        token::Client::new(&env, &token).transfer(&sender, &env.current_contract_address(), &total);

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);
        let stream = Stream {
            sender: sender.clone(),
            recipient: recipient.clone(),
            token: token.clone(),
            total,
            start,
            cliff,
            end,
            withdrawn: 0,
            cancelled: false,
        };
        env.storage().persistent().set(&DataKey::Stream(id), &stream);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        // Maintain enumeration indexes for paginated views.
        let count: u64 = env.storage().instance().get(&DataKey::StreamCount).unwrap_or(0);
        env.storage().instance().set(&DataKey::StreamCount, &(count + 1));

        let sender_key = DataKey::SenderStreams(sender.clone());
        let mut sender_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&sender_key)
            .unwrap_or_else(|| Vec::new(&env));
        sender_ids.push_back(id);
        env.storage().persistent().set(&sender_key, &sender_ids);

        let recipient_key = DataKey::RecipientStreams(recipient.clone());
        let mut recipient_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&recipient_key)
            .unwrap_or_else(|| Vec::new(&env));
        recipient_ids.push_back(id);
        env.storage().persistent().set(&recipient_key, &recipient_ids);

        env.events().publish(
            (soroban_sdk::symbol_short!("create"), id),
            (sender, recipient, token, total, start, cliff, end),
        );
        Ok(id)
    }

    /// Pull-based withdrawal of all currently vested funds for a stream.
    pub fn withdraw(env: Env, stream_id: u64) -> Result<i128, Error> {
        if Self::is_paused(&env) {
            return Err(Error::Paused);
        }

        let mut stream: Stream = env
            .storage()
            .persistent()
            .get(&DataKey::Stream(stream_id))
            .ok_or(Error::StreamNotFound)?;

        stream.recipient.require_auth();

        let vested = Self::vested_amount(&stream, env.ledger().timestamp());
        let claimable = vested - stream.withdrawn;
        if claimable <= 0 {
            return Err(Error::NothingToWithdraw);
        }

        stream.withdrawn = vested;
        env.storage().persistent().set(&DataKey::Stream(stream_id), &stream);

        token::Client::new(&env, &stream.token).transfer(
            &env.current_contract_address(),
            &stream.recipient,
            &claimable,
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("withdraw"), stream_id),
            (stream.recipient.clone(), claimable),
        );
        Ok(claimable)
    }

    /// Cancels a stream. Unvested funds return to the sender; vested funds stay
    /// withdrawable by the recipient. Only the governor may call this.
    ///
    /// Intentionally NOT gated by the pause flag: cancellation returns funds to
    /// senders and must remain available during an incident.
    pub fn cancel(env: Env, stream_id: u64) -> Result<(), Error> {
        let governor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Governor)
            .ok_or(Error::NotInitialized)?;
        governor.require_auth();

        let mut stream: Stream = env
            .storage()
            .persistent()
            .get(&DataKey::Stream(stream_id))
            .ok_or(Error::StreamNotFound)?;
        if stream.cancelled {
            return Err(Error::AlreadyCancelled);
        }

        let vested = Self::vested_amount(&stream, env.ledger().timestamp());
        let unvested = stream.total - vested;

        stream.cancelled = true;
        // Cap the stream at what has vested so far; the recipient keeps the
        // right to withdraw the vested remainder.
        stream.total = vested;
        env.storage().persistent().set(&DataKey::Stream(stream_id), &stream);

        if unvested > 0 {
            token::Client::new(&env, &stream.token).transfer(
                &env.current_contract_address(),
                &stream.sender,
                &unvested,
            );
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("cancel"), stream_id),
            (stream.sender.clone(), unvested),
        );
        Ok(())
    }

    /// Returns the currently withdrawable (vested but not yet withdrawn) balance.
    pub fn balance_of(env: Env, stream_id: u64) -> Result<i128, Error> {
        let stream: Stream = env
            .storage()
            .persistent()
            .get(&DataKey::Stream(stream_id))
            .ok_or(Error::StreamNotFound)?;
        let vested = Self::vested_amount(&stream, env.ledger().timestamp());
        Ok(vested - stream.withdrawn)
    }

    /// Returns the full stream record.
    pub fn get_stream(env: Env, stream_id: u64) -> Result<Stream, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Stream(stream_id))
            .ok_or(Error::StreamNotFound)
    }

    /// Returns the total number of streams ever created.
    pub fn get_stream_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::StreamCount).unwrap_or(0)
    }

    /// Returns a page of stream ids created by `sender`, starting at `start`
    /// with at most `limit` entries. Out-of-range `start` yields an empty page.
    pub fn list_streams_by_sender(env: Env, sender: Address, start: u32, limit: u32) -> Vec<u64> {
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::SenderStreams(sender))
            .unwrap_or_else(|| Vec::new(&env));
        Self::paginate(&env, &ids, start, limit)
    }

    /// Returns a page of stream ids created for `recipient`, starting at
    /// `start` with at most `limit` entries. Out-of-range `start` yields an
    /// empty page.
    pub fn list_streams_by_recipient(
        env: Env,
        recipient: Address,
        start: u32,
        limit: u32,
    ) -> Vec<u64> {
        let ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::RecipientStreams(recipient))
            .unwrap_or_else(|| Vec::new(&env));
        Self::paginate(&env, &ids, start, limit)
    }

    /// Slices `ids` into a page of at most `limit` entries beginning at `start`.
    fn paginate(env: &Env, ids: &Vec<u64>, start: u32, limit: u32) -> Vec<u64> {
        let mut page = Vec::new(env);
        let len = ids.len();
        if start >= len || limit == 0 {
            return page;
        }
        let end = core::cmp::min(start.saturating_add(limit), len);
        let mut i = start;
        while i < end {
            page.push_back(ids.get_unchecked(i));
            i += 1;
        }
        page
    }

    /// Loads the governor and requires its authorization.
    fn require_governor(env: &Env) -> Result<Address, Error> {
        let governor: Address = env
            .storage()
            .instance()
            .get(&DataKey::Governor)
            .ok_or(Error::NotInitialized)?;
        governor.require_auth();
        Ok(governor)
    }

    /// Precision-safe vested amount: `total * elapsed / duration` using integer
    /// math. The final withdrawal sweeps any rounding dust because once
    /// `now >= end` the full `total` is considered vested.
    fn vested_amount(stream: &Stream, now: u64) -> i128 {
        if now < stream.cliff {
            return 0;
        }
        if now >= stream.end {
            return stream.total;
        }
        let duration = (stream.end - stream.start) as i128;
        let elapsed = (now - stream.start) as i128;
        match stream
            .total
            .checked_mul(elapsed)
            .and_then(|n| n.checked_div(duration))
        {
            Some(vested) => vested,
            None => panic_with_error!(&Env::default(), Error::ArithmeticOverflow),
        }
    }
}

#[cfg(test)]
mod test;
