"""TLS front-end for osu-echo's --legacy-tls mode.

Terminates TLS 1.0/1.1 (and 1.2+) and pipes the decrypted bytes to the server's plain HTTP
listener. It exists because an osu! client from 2022 or earlier only offers TLS 1.0/1.1, which
rustls refuses and which Windows' own TLS stack will not negotiate unless a machine-wide registry
policy is relaxed. Python's ssl module carries its own OpenSSL, so nothing outside this process
is affected and Windows' TLS settings stay exactly as they are.

The pipe is byte-for-byte, so the server sees the client's original request, including the Host
header it uses for subdomain routing.

Started by osu-echo itself; see `--legacy-tls`. Not meant to be run by hand.
"""

import ctypes
import os
import socket
import ssl
import sys
import threading

PARENT_PID = os.getppid()

_kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
_SYNCHRONIZE = 0x00100000
_WAIT_TIMEOUT = 0x00000102
_ERROR_FILE_NOT_FOUND = 2
_ERROR_ACCESS_DENIED = 5
_ERROR_INVALID_PARAMETER = 87


def parent_alive() -> bool:
    """False once osu-echo is gone, so this proxy cannot outlive it holding port 443.

    Windows does not reparent orphans, so os.getppid() keeps naming the dead parent and cannot be
    used for this. Asking the kernel whether that process still exists can, but the answer is in
    two places: WaitForSingleObject when the process is still there, and the error code when it is
    not. Treating a failed OpenProcess as "still alive" is what would keep the port bound, since
    the PID of a fully exited process is normally gone and OpenProcess simply reports it invalid.
    """
    handle = _kernel32.OpenProcess(_SYNCHRONIZE, False, PARENT_PID)
    if handle:
        try:
            return _kernel32.WaitForSingleObject(handle, 0) == _WAIT_TIMEOUT
        finally:
            _kernel32.CloseHandle(handle)
    error = ctypes.get_last_error()
    if error in (_ERROR_FILE_NOT_FOUND, _ERROR_INVALID_PARAMETER):
        return False  # the parent no longer exists
    return error != _ERROR_ACCESS_DENIED  # anything else (incl. access denied): assume alive


def main() -> int:
    args = dict(zip(sys.argv[1::2], sys.argv[2::2]))
    listen_host = args.get("--listen-host", "127.0.0.1")
    listen_port = int(args.get("--listen-port", "443"))
    upstream_host = args.get("--upstream-host", "127.0.0.1")
    upstream_port = int(args.get("--upstream-port", "5000"))
    cert = args["--cert"]
    key = args["--key"]

    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    # TLS 1.0/1.1 only have CBC suites with SHA-1 MACs, which OpenSSL 3 excludes at its default
    # security level. Safe here because this listener only ever binds loopback.
    context.minimum_version = ssl.TLSVersion.TLSv1
    context.set_ciphers("ALL:@SECLEVEL=0")
    context.load_cert_chain(certfile=cert, keyfile=key)

    def pump(src, dst):
        try:
            while True:
                data = src.recv(65536)
                if not data:
                    break
                dst.sendall(data)
        except OSError:
            pass
        finally:
            for s in (src, dst):
                try:
                    s.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                s.close()

    def handle(client):
        try:
            tls = context.wrap_socket(client, server_side=True)
        except (ssl.SSLError, OSError) as exc:
            print("handshake failed: %s" % exc, flush=True)
            return
        try:
            upstream = socket.create_connection((upstream_host, upstream_port), timeout=30)
        except OSError as exc:
            print("upstream unreachable: %s" % exc, flush=True)
            tls.close()
            return
        threading.Thread(target=pump, args=(tls, upstream), daemon=True).start()
        pump(upstream, tls)

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((listen_host, listen_port))
    listener.listen(128)
    # A timeout on accept is what makes the liveness check below fire while the server is idle:
    # blocking forever in accept() would leave port 443 held until the next client connects.
    listener.settimeout(2.0)
    print("legacy TLS listening on %s:%d -> %s:%d" % (listen_host, listen_port, upstream_host, upstream_port), flush=True)

    while True:
        if not parent_alive():
            # osu-echo was killed; leaving would keep port 443 busy for the next start.
            print("osu-echo is gone, shutting down", flush=True)
            break
        try:
            client, _ = listener.accept()
        except socket.timeout:
            continue
        except OSError:
            break
        if not parent_alive():
            client.close()
            break
        threading.Thread(target=handle, args=(client,), daemon=True).start()
    return 0


if __name__ == "__main__":
    sys.exit(main())
