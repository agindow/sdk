/** Customer-owned signer example; cryptography is a host dependency, not an SDK requirement. */
export const keylessPythonSetup = `from base64 import urlsafe_b64encode
from pathlib import Path
from time import time
from uuid import uuid4
import json
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from agindow import ProofSigner, RemoteAuth, workload_identity


def connect_self_hosted(*, exchange_endpoint: str, registration: str,
                        client_id: str, kid: str, audience: str,
                        private_key_file: str, tools: tuple[str, ...]) -> RemoteAuth:
    # This file is read only by the customer's workload, never uploaded.
    key = serialization.load_pem_private_key(Path(private_key_file).read_bytes(), password=None)
    if not isinstance(key, Ed25519PrivateKey):
        raise ValueError("Ed25519 client key required")

    def encode(value: object) -> str:
        return urlsafe_b64encode(json.dumps(value, separators=(",", ":")).encode()).rstrip(b"=").decode()

    def assertion() -> str:
        now = int(time())
        header = encode({"alg": "EdDSA", "typ": "JWT", "kid": kid})
        claims = encode({"iss": client_id, "sub": client_id, "aud": audience,
                         "iat": now, "nbf": now, "exp": now + 120, "jti": str(uuid4())})
        value = header + "." + claims
        signature = urlsafe_b64encode(key.sign(value.encode())).rstrip(b"=").decode()
        return value + "." + signature

    proof_key = Ed25519PrivateKey.generate()
    public_x = urlsafe_b64encode(proof_key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw)).rstrip(b"=").decode()
    signer = ProofSigner(public_x, proof_key.sign)
    return RemoteAuth(workload_identity(exchange_endpoint=exchange_endpoint,
                      registration=registration, tools=tools, assertion=assertion), signer)
`;
export const keylessGenerate = `import { generateKeyPairSync, randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
const { privateKey, publicKey } = generateKeyPairSync('ed25519');
// Refuse to overwrite an existing workload key. Keep this file out of git and model context.
await writeFile('./workload-private.pem', privateKey.export({type:'pkcs8',format:'pem'}), {mode:0o600,flag:'wx'});
console.log(JSON.stringify({kid:randomUUID(),x:publicKey.export({format:'jwk'}).x}));`;
