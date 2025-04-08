# structured short message decomposition
speaker: <string>
type:                       # One of:
  - assertion
  - judgment
  - attribution
  - feeling
  - question
  - call_to_action
target:
  entity_type: <string>     # e.g., person, company, policy
  entity_id: <string>       # unique label or reference
judgment:
  dimension: <string>       # e.g., competence, honesty, harm
  score: <float>            # range from -5.0 (strongly negative) to +5.0 (strongly positive)
  confidence: <float>       # 0.0 to 1.0
evidence:
  - description: <string>
    source: <string>
feeling:
  emotion: <string>         # e.g., joy, fear, pride
  valence: <float>          # -5.0 to +5.0, how strongly the emotion is felt
  target: self | other | entity
question:
  about: <string>
action:
  recommendation: <string>
  strength: <float>         # -5.0 = strong rejection, +5.0 = strong endorsement
