// A React-based web client for the prioritization room
import React, { useState, useEffect } from 'react';
import { useQuery, useMutation, gql } from '@apollo/client';

const GET_CURRENT_ROUND = gql`
  query GetCurrentRound {
    currentRound {
      id
      startTime
      endTime
      status
    }
  }
`;

const GET_CURRENT_PAIRING = gql`
  query GetCurrentPairing($roundId: ID!) {
    currentPairing(roundId: $roundId) {
      id
      topicA {
        id
        title
        description
      }
      topicB {
        id
        title
        description
      }
    }
  }
`;

const GET_TOP_TOPICS = gql`
  query GetTopTopics($limit: Int) {
    topTopics(limit: $limit) {
      topicId
      rank
      score
      topic {
        id
        title
        description
      }
    }
  }
`;

const SUBMIT_VOTE = gql`
  mutation SubmitVote($pairingId: ID!, $userId: ID!, $choice: ID!) {
    submitVote(pairingId: $pairingId, userId: $userId, choice: $choice)
  }
`;

function App() {
  const [userId] = useState(() => {
    // Generate a random UUID for this user session
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0, v = c === 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    });
  });
  
  const [selectedTopic, setSelectedTopic] = useState(null);
  
  // Query for the current round
  const { data: roundData, loading: roundLoading } = useQuery(GET_CURRENT_ROUND, {
    pollInterval: 1000,
  });
  
  // Query for current pairing when round is available
  const { data: pairingData, loading: pairingLoading } = useQuery(GET_CURRENT_PAIRING, {
    skip: !roundData?.currentRound?.id,
    variables: { 
      roundId: roundData?.currentRound?.id 
    },
    pollInterval: 1000,
  });
  
  // Query for top topics
  const { data: topicsData } = useQuery(GET_TOP_TOPICS, {
    variables: { limit: 10 },
    pollInterval: 5000,
  });
  
  // Mutation for submitting votes
  const [submitVote] = useMutation(SUBMIT_VOTE);
  
  // Calculate remaining time in round
  const [remainingTime, setRemainingTime] = useState(0);
  
  useEffect(() => {
    if (roundData?.currentRound) {
      const endTime = new Date(roundData.currentRound.endTime).getTime();
      
      const timer = setInterval(() => {
        const now = new Date().getTime();
        const remaining = Math.max(0, endTime - now);
        setRemainingTime(Math.floor(remaining / 1000));
        
        if (remaining <= 0) {
          clearInterval(timer);
        }
      }, 1000);
      
      return () => clearInterval(timer);
    }
  }, [roundData]);
  
  // Handle vote submission
  const handleVote = (topicId) => {
    if (!pairingData?.currentPairing?.id) return;
    
    submitVote({
      variables: {
        pairingId: pairingData.currentPairing.id,
        userId: userId,
        choice: topicId
      }
    });
    
    setSelectedTopic(topicId);
  };
  
  // Format time remaining
  const formatTime = (seconds) => {
    return `${Math.floor(seconds / 60)}:${(seconds % 60).toString().padStart(2, '0')}`;
  };
  
  return (
    <div className="container mx-auto px-4 py-8 max-w-4xl">
      <h1 className="text-3xl font-bold mb-8 text-center">Prioritization Room</h1>
      
      {/* Round Information */}
      <div className="mb-8 p-4 bg-gray-100 rounded-lg">
        <h2 className="text-xl font-semibold mb-2">Current Round</h2>
        {roundLoading ? (
          <p>Loading round information...</p>
        ) : roundData?.currentRound ? (
          <div>
            <p className="mb-2">Round ID: {roundData.currentRound.id.substring(0, 8)}...</p>
            <p className="mb-2">Status: {roundData.currentRound.status}</p>
            <p className="text-xl font-bold">Time remaining: {formatTime(remainingTime)}</p>
          </div>
        ) : (
          <p>No active round at the moment. Please wait...</p>
        )}
      </div>
      
      {/* Pairing Vote Interface */}
      {roundData?.currentRound && (
        <div className="mb-8">
          <h2 className="text-xl font-semibold mb-4">Which issue is more important?</h2>
          
          {pairingLoading ? (
            <p>Loading current topics...</p>
          ) : pairingData?.currentPairing ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {/* Topic A */}
              <div 
                className={`p-4 border rounded-lg cursor-pointer transition-all ${
                  selectedTopic === pairingData.currentPairing.topicA.id
                    ? 'border-4 border-blue-500 bg-blue-50'
                    : 'border hover:border-blue-300'
                }`}
                onClick={() => handleVote(pairingData.currentPairing.topicA.id)}
              >
                <h3 className="text-lg font-semibold mb-2">{pairingData.currentPairing.topicA.title}</h3>
                <p className="text-gray-600">{pairingData.currentPairing.topicA.description}</p>
              </div>
              
              {/* Topic B */}
              <div 
                className={`p-4 border rounded-lg cursor-pointer transition-all ${
                  selectedTopic === pairingData.currentPairing.topicB.id
                    ? 'border-4 border-blue-500 bg-blue-50'
                    : 'border hover:border-blue-300'
                }`}
                onClick={() => handleVote(pairingData.currentPairing.topicB.id)}
              >
                <h3 className="text-lg font-semibold mb-2">{pairingData.currentPairing.topicB.title}</h3>
                <p className="text-gray-600">{pairingData.currentPairing.topicB.description}</p>
              </div>
            </div>
          ) : (
            <p>No topics to compare at the moment.</p>
          )}
        </div>
      )}
      
      {/* Top Topics Ranking */}
      <div>
        <h2 className="text-xl font-semibold mb-4">Current Topic Rankings</h2>
        
        {topicsData?.topTopics ? (
          <div className="overflow-x-auto">
            <table className="min-w-full bg-white border">
              <thead>
                <tr>
                  <th className="py-2 px-4 border-b">Rank</th>
                  <th className="py-2 px-4 border-b">Topic</th>
                  <th className="py-2 px-4 border-b">Score</th>
                </tr>
              </thead>
              <tbody>
                {topicsData.topTopics.map((item) => (
                  <tr key={item.topicId}>
                    <td className="py-2 px-4 border-b text-center">{item.rank}</td>
                    <td className="py-2 px-4 border-b">
                      <div>
                        <div className="font-medium">{item.topic.title}</div>
                        <div className="text-sm text-gray-500">{item.topic.description}</div>
                      </div>
                    </td>
                    <td className="py-2 px-4 border-b text-center">{Math.round(item.score)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p>Loading ranking data...</p>
        )}
      </div>
      
      <div className="mt-8 text-sm text-gray-500 text-center">
        User ID: {userId.substring(0, 8)}...
      </div>
    </div>
  );
}

export default App;