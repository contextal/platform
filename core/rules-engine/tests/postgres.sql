CREATE TABLE objects (
    id bigserial NOT NULL PRIMARY KEY,
    org text NOT NULL,
    work_id text NOT NULL,
    is_entry boolean NOT NULL,
    object_id text NOT NULL,
    object_type text NOT NULL,
    object_subtype text NULL,
    recursion_level integer NOT NULL,
    size bigint NOT NULL,
    hashes jsonb NOT NULL,
    t timestamp with time zone NOT NULL,
    result jsonb NOT NULL
);
CREATE INDEX o_work_id_idx ON objects USING hash (work_id);
CREATE INDEX o_object_id_idx ON objects USING hash (object_id);
CREATE INDEX o_object_type_idx ON objects USING hash (object_type);
CREATE INDEX o_t_idx ON objects USING btree (t);
CREATE INDEX o_result_idx ON objects USING GIN (result);

CREATE TABLE rels (
    parent bigint NULL REFERENCES objects(id) ON DELETE CASCADE ON UPDATE CASCADE,
    child bigint NOT NULL REFERENCES objects(id) ON DELETE CASCADE ON UPDATE CASCADE,
    props jsonb NOT NULL
);
CREATE INDEX r_parent_idx ON rels USING hash (parent);
CREATE INDEX r_child_idx ON rels USING hash (child);
CREATE INDEX r_props_idx ON rels USING GIN (props);

CREATE OR REPLACE FUNCTION public.descendants_of(reference_id bigint, min_distance integer DEFAULT 1, max_distance integer DEFAULT 100000)
 RETURNS SETOF bigint
 LANGUAGE sql
 STABLE PARALLEL SAFE STRICT ROWS 10
AS $function$
  WITH RECURSIVE rchildren(id, depth) AS (
    SELECT reference_id, 0
  UNION ALL
    SELECT child, rchildren.depth + 1 FROM rels, rchildren WHERE rels.parent = rchildren.id
  )
  SELECT id FROM rchildren WHERE depth BETWEEN min_distance AND max_distance;
$function$;

CREATE OR REPLACE FUNCTION public.ancestors_of(reference_id bigint, min_distance integer DEFAULT 1, max_distance integer DEFAULT 100000)
 RETURNS SETOF bigint
 LANGUAGE sql
 STABLE PARALLEL SAFE STRICT ROWS 4
AS $function$
  WITH RECURSIVE rchildren(id, depth) AS (
    SELECT reference_id, 0
  UNION ALL
    SELECT parent, rchildren.depth + 1 FROM rels, rchildren WHERE rels.child = rchildren.id
  )
  SELECT id FROM rchildren WHERE depth BETWEEN min_distance AND max_distance;
$function$;