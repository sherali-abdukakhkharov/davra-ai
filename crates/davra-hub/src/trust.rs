//! Trust is always relative to the reader: the same message is `owner` for
//! one agent and `peer_human` for another.

use davra_core::api::{MemberKind, Trust};
use uuid::Uuid;

pub struct Reader {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
}

pub struct Author {
    pub id: Option<Uuid>,
    pub kind: Option<MemberKind>,
    pub owner_id: Option<Uuid>,
}

pub fn trust_for(reader: &Reader, author: &Author) -> Trust {
    let Some(author_id) = author.id else {
        return Trust::External;
    };
    if author_id == reader.id {
        return Trust::Myself;
    }
    if reader.owner_id == Some(author_id) {
        return Trust::Owner;
    }
    match author.kind {
        // Another agent with the same owner (e.g. the owner's other laptop),
        // or a human reading their own agent.
        Some(MemberKind::Agent)
            if (author.owner_id.is_some() && author.owner_id == reader.owner_id)
                || author.owner_id == Some(reader.id) =>
        {
            Trust::Myself
        }
        Some(MemberKind::Agent) => Trust::PeerAgent,
        Some(MemberKind::Human) => Trust::PeerHuman,
        None => Trust::External,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder() {
        let lionali = Uuid::new_v4();
        let gulom = Uuid::new_v4();
        let lionali_ai = Reader {
            id: Uuid::new_v4(),
            owner_id: Some(lionali),
        };
        let gulom_ai = Uuid::new_v4();
        let lionali_ai2 = Uuid::new_v4();

        let human = |id| Author {
            id: Some(id),
            kind: Some(MemberKind::Human),
            owner_id: None,
        };
        let agent = |id, owner| Author {
            id: Some(id),
            kind: Some(MemberKind::Agent),
            owner_id: Some(owner),
        };

        assert_eq!(trust_for(&lionali_ai, &human(lionali)), Trust::Owner);
        assert_eq!(trust_for(&lionali_ai, &human(gulom)), Trust::PeerHuman);
        assert_eq!(
            trust_for(&lionali_ai, &agent(gulom_ai, gulom)),
            Trust::PeerAgent
        );
        assert_eq!(
            trust_for(&lionali_ai, &agent(lionali_ai2, lionali)),
            Trust::Myself
        );
        assert_eq!(
            trust_for(&lionali_ai, &agent(lionali_ai.id, lionali)),
            Trust::Myself
        );
        assert_eq!(
            trust_for(
                &lionali_ai,
                &Author {
                    id: None,
                    kind: None,
                    owner_id: None
                }
            ),
            Trust::External
        );
        // A human reading their own agent.
        let lionali_reader = Reader {
            id: lionali,
            owner_id: None,
        };
        assert_eq!(
            trust_for(&lionali_reader, &agent(lionali_ai2, lionali)),
            Trust::Myself
        );
        assert_eq!(
            trust_for(&lionali_reader, &agent(gulom_ai, gulom)),
            Trust::PeerAgent
        );
    }
}
