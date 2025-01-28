LOCK TABLE objects;
UPDATE objects SET object_type = 'LNK' WHERE object_type = 'Lnk';
UPDATE objects SET object_type = 'PDF' WHERE object_type = 'Pdf';
UPDATE objects SET object_type = 'RAR' WHERE object_type = 'Rar';
UPDATE objects SET object_type = 'URL' WHERE object_type = 'Url';
UPDATE objects SET object_type = 'ZIP' WHERE object_type = 'Zip';
