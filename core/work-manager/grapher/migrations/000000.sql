CREATE TABLE IF NOT EXISTS objects (
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
CREATE INDEX IF NOT EXISTS o_work_id_idx ON objects USING hash (work_id);
CREATE INDEX IF NOT EXISTS o_object_id_idx ON objects USING hash (object_id);
CREATE INDEX IF NOT EXISTS o_object_type_idx ON objects USING hash (object_type);
CREATE INDEX IF NOT EXISTS o_t_idx ON objects USING btree (t);
CREATE INDEX IF NOT EXISTS o_result_idx ON objects USING GIN (result);

CREATE TABLE IF NOT EXISTS rels (
    parent bigint NULL REFERENCES objects(id) ON DELETE CASCADE ON UPDATE CASCADE,
    child bigint NOT NULL REFERENCES objects(id) ON DELETE CASCADE ON UPDATE CASCADE,
    props jsonb NOT NULL
);
CREATE INDEX IF NOT EXISTS r_parent_idx ON rels USING hash (parent);
CREATE INDEX IF NOT EXISTS r_child_idx ON rels USING hash (child);
CREATE INDEX IF NOT EXISTS r_props_idx ON rels USING GIN (props);

CREATE TABLE IF NOT EXISTS scenarios (
    id bigserial NOT NULL PRIMARY KEY,
    name text NOT NULL UNIQUE,
    t timestamptz NOT NULL DEFAULT current_timestamp,
    def jsonb NOT NULL,
    CONSTRAINT same_name CHECK (name = def->>'name')
);

CREATE TABLE IF NOT EXISTS results (
    id bigserial NOT NULL PRIMARY KEY,
    work_id text NOT NULL,
    t timestamptz NOT NULL DEFAULT current_timestamp,
    actions jsonb NOT NULL
);
CREATE INDEX IF NOT EXISTS res_work_id_idx ON results USING hash (work_id);

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
