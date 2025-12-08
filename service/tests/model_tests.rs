use async_graphql::ID;
use chrono::Utc;
use tinycongress_api::graphql::{Pairing, Round, Topic, TopicRanking};

#[test]
fn test_round_creation() {
    let now = Utc::now();
    let end_time = now + chrono::Duration::minutes(10);

    let round = Round {
        id: ID::from("test-round-1"),
        start_time: now.to_rfc3339(),
        end_time: end_time.to_rfc3339(),
        status: "active".to_string(),
    };

    assert_eq!(round.id, ID::from("test-round-1"));
    assert_eq!(round.status, "active");
}

#[test]
fn test_topic_creation() {
    let topic = Topic {
        id: ID::from("topic-1"),
        title: "Climate Change".to_string(),
        description: "Address global climate change and its impacts".to_string(),
    };

    assert_eq!(topic.id, ID::from("topic-1"));
    assert_eq!(topic.title, "Climate Change");
    assert_eq!(
        topic.description,
        "Address global climate change and its impacts"
    );
}

#[test]
fn test_pairing_creation() {
    let topic_a = Topic {
        id: ID::from("topic-1"),
        title: "Climate Change".to_string(),
        description: "Address global climate change and its impacts".to_string(),
    };

    let topic_b = Topic {
        id: ID::from("topic-2"),
        title: "Healthcare Reform".to_string(),
        description: "Improve healthcare access and affordability".to_string(),
    };

    let pairing = Pairing {
        id: ID::from("pairing-1"),
        topic_a,
        topic_b,
    };

    assert_eq!(pairing.id, ID::from("pairing-1"));
    assert_eq!(pairing.topic_a.id, ID::from("topic-1"));
    assert_eq!(pairing.topic_b.id, ID::from("topic-2"));
}

#[test]
fn test_topic_ranking() {
    let topic = Topic {
        id: ID::from("topic-1"),
        title: "Climate Change".to_string(),
        description: "Address global climate change and its impacts".to_string(),
    };

    let ranking = TopicRanking {
        topic_id: ID::from("topic-1"),
        rank: 1,
        score: 1550.0,
        topic,
    };

    assert_eq!(ranking.topic_id, ID::from("topic-1"));
    assert_eq!(ranking.rank, 1);
    assert_eq!(ranking.score, 1550.0);
    assert_eq!(ranking.topic.title, "Climate Change");
}
