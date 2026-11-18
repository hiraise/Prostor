-- Rename the text search configuration of D-037 with the product name again (D-066). The
-- generated columns still follow the configuration by OID.

alter text search configuration taskmaster_text rename to prostor_text;
