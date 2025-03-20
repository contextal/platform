LOCK TABLE scenarios;
UPDATE scenarios SET def['context']['global_query'] = to_jsonb(E'MATCHES: >10%;\nTIME_WINDOW: 1 day;\nMAX_NEIGHBORS: ' || (def->'context'->'min_matches')::numeric * 10 || E';\n' || (def->'context'->>'global_query'))
  WHERE def @@ '$.context != null';
UPDATE scenarios SET def = jsonb_set(def #- '{max_ver}' #- '{min_ver}' #- '{context,min_matches}', '{compatible_with}', '">=1.3.0"');
