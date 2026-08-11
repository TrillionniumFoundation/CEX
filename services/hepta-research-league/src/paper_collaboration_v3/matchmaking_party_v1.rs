use super::{MatchmakingTicket, MatchmakingTicketStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct MatchmakingPartitionKey<'a> {
    challenge_id: uuid::Uuid,
    availability_hash: &'a str,
    party_code_hash: Option<&'a str>,
}

pub(super) fn matchmaking_partition_key(ticket: &MatchmakingTicket) -> MatchmakingPartitionKey<'_> {
    MatchmakingPartitionKey {
        challenge_id: ticket.challenge_id,
        availability_hash: ticket.availability_hash.as_str(),
        party_code_hash: ticket.party_code_hash.as_deref(),
    }
}

pub(super) fn matchmaking_partition_compatible(
    left: &MatchmakingTicket,
    right: &MatchmakingTicket,
) -> bool {
    matchmaking_partition_key(left) == matchmaking_partition_key(right)
}

pub(super) fn live_party_ticket_count<'a>(
    tickets: impl Iterator<Item = &'a MatchmakingTicket>,
    challenge_id: uuid::Uuid,
    party_code_hash: &str,
) -> usize {
    tickets
        .filter(|ticket| {
            ticket.challenge_id == challenge_id
                && ticket.party_code_hash.as_deref() == Some(party_code_hash)
                && matches!(
                    ticket.status,
                    MatchmakingTicketStatus::Queued | MatchmakingTicketStatus::Matched
                )
        })
        .count()
}

pub(super) fn postgres_party_admission_lock_key(
    challenge_id: uuid::Uuid,
    party_code_hash: &str,
) -> String {
    let partition = format!("{challenge_id}:{party_code_hash}");
    format!(
        "hepta-paper-raid-matchmaking-party:{}",
        crate::paper_raid_contracts::sha256_digest(partition.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    use super::*;

    fn ticket(party_code_hash: Option<&str>, status: MatchmakingTicketStatus) -> MatchmakingTicket {
        let now = Utc::now();
        MatchmakingTicket {
            ticket_id: Uuid::new_v4(),
            player_id: Uuid::new_v4(),
            challenge_id: Uuid::nil(),
            requested_team_size: 3,
            roles: vec!["captain".to_string()],
            availability_hash:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
            party_code_hash: party_code_hash.map(str::to_string),
            status,
            matched_proposal_id: None,
            expires_at: Some(now + Duration::minutes(30)),
            queue_hint: None,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn public_and_private_partitions_never_compare_equal() {
        let public = ticket(None, MatchmakingTicketStatus::Queued);
        let party_a = ticket(
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            MatchmakingTicketStatus::Queued,
        );
        let party_a_peer = ticket(
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            MatchmakingTicketStatus::Queued,
        );
        let party_b = ticket(
            Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
            MatchmakingTicketStatus::Queued,
        );
        let mut other_challenge = party_a_peer.clone();
        other_challenge.challenge_id = Uuid::new_v4();

        assert!(!matchmaking_partition_compatible(&public, &party_a));
        assert!(matchmaking_partition_compatible(&party_a, &party_a_peer));
        assert!(!matchmaking_partition_compatible(&party_a, &party_b));
        assert!(!matchmaking_partition_compatible(
            &party_a,
            &other_challenge
        ));
    }

    #[test]
    fn live_party_count_excludes_terminal_and_other_challenge_tickets() {
        let mut queued = ticket(
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            MatchmakingTicketStatus::Queued,
        );
        let matched = ticket(
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            MatchmakingTicketStatus::Matched,
        );
        let cancelled = ticket(
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            MatchmakingTicketStatus::Cancelled,
        );
        let mut other_challenge = ticket(
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            MatchmakingTicketStatus::Queued,
        );
        queued.challenge_id = matched.challenge_id;
        other_challenge.challenge_id = Uuid::new_v4();
        let tickets = [queued, matched.clone(), cancelled, other_challenge];

        assert_eq!(
            live_party_ticket_count(
                tickets.iter(),
                matched.challenge_id,
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
            2,
        );
    }

    #[test]
    fn postgres_party_admission_lock_is_partition_specific_and_hides_the_party_hash() {
        let challenge_id = Uuid::new_v4();
        let party_a = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let party_b = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let key = postgres_party_admission_lock_key(challenge_id, party_a);

        assert!(key.starts_with("hepta-paper-raid-matchmaking-party:sha256:"));
        assert!(!key.contains(party_a));
        assert_ne!(
            key,
            postgres_party_admission_lock_key(challenge_id, party_b)
        );
        assert_ne!(
            key,
            postgres_party_admission_lock_key(Uuid::new_v4(), party_a)
        );
    }
}
