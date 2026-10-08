# Temporary patch: re-review Required fixes — batch 1 (s3compat + storage).

# ---- 1. s3.rs sign_request: host WITH port (R-16 completion) ----
p = 'crates/storage/src/driver/s3.rs'
src = open(p, encoding='utf-8').read()
import re
# find the header-signing host derivation around line 244
old = '''        let host = url
            .host_str()
            .ok_or_else(|| StorageError::Config("missing host".into()))?'''
assert old in src, 'sign_request host'
src = src.replace(old, '''        let host = host_with_port(url)?;''')
open(p, 'w', encoding='utf-8', newline='\n').write(src)
print("sign_request host fixed")
