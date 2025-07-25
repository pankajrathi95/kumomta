# Unreleased Changes in The Mainline

## Breaking Changes

## Other Changes and Enhancements

 * [try_next_host_on_transport_error](../reference/kumo/make_egress_path/try_next_host_on_transport_error.md)
   option to more aggressively retry failures that are either transport errors
   (eg: timeout) or are not definitively associated with the message (eg:
   response to commands in between transactions).
 * You may now specify outbound SMTP port numbers when configuring
   [make_queue_config().protocol](../reference/kumo/make_queue_config/protocol.md)
   with an `mx_list`.
 * You may now specify outbound SMTP port numbers when assigning either the
   `routing_domain` or the domain portion of the scheduled queue name using the
   `queue` meta item. #352
 * [kumo.dns.lookup_ptr](../reference/kumo.dns/lookup_ptr.md) function for looking
   up PTR records. Thanks to @kayozaki! #390
 * [kumo.mpsc.define](../reference/kumo.mpsc/define.md) function for advanced
   non-durable, non-persistent, in-memory queue processing.
 * [kumo.fs](../reference/kumo.fs/index.md) module for efficiently working with
   the filesystem.  The functions
   [kumo.read_dir](../reference/kumo/read_dir.md),
   [kumo.glob](../reference/kumo/glob.md) and
   [kumo.uncached_glob](../reference/kumo/uncached_glob.md) have been
   deprecated in favor of functions with the same names in `kumo.fs`.  In
   addition, a new [kumo.fs.open](../reference/kumo.fs/open.md) function that
   can create async capable file handles is now provided.
 * SMTP Receptions made via TLS now: #100
    * Show in the trace headers as ESMTPS/ESMTPSA along with the TLS version
      and cipher as a comment. eg: `with ESMTPS (TLSv1_3:TLS13_AES_256_GCM_SHA384)`
    * Are recorded as `tls_cipher`, `tls_protocol_version` and
      `tls_peer_subject_name` in the meta values for the message and in the
      `Reception` log record.
 * New
   [tls_required_client_ca](../reference/kumo/start_esmtp_listener/tls_required_client_ca.md)
   parameter to aid in configuring mTLS

## Fixes

 * `msg:check_fix_conformance` could panic when attempting to fix messages with
   broken base64 parts
