-- Rename the text search configuration of D-037 together with the product name (D-065). The
-- generated columns of 0010 reference the configuration by OID and follow the rename.

alter text search configuration trackflow_text rename to taskmaster_text;
