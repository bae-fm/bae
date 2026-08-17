use jni::{objects::JObject, sys::jstring, JNIEnv};

/// Installs Android's certificate verifier before any cloud client can make a
/// TLS connection. The matching JVM component is packaged from the Rust
/// crate's own Maven directory, so the native and JVM halves advance together.
#[unsafe(no_mangle)]
pub(crate) extern "system" fn Java_fm_bae_app_AndroidRuntime_initializeTls(
    mut env: JNIEnv,
    _runtime: JObject,
    context: JObject,
) -> jstring {
    match rustls_platform_verifier::android::init_with_env(&mut env, context) {
        Ok(()) => std::ptr::null_mut(),
        Err(error) => match env.new_string(error.to_string()) {
            Ok(message) => message.into_raw(),
            Err(string_error) => {
                tracing::error!(%error, %string_error, "Android TLS verifier initialization failed");
                if let Err(throw_error) = env.throw_new(
                    "java/lang/IllegalStateException",
                    "Android TLS verifier initialization failed",
                ) {
                    tracing::error!(%throw_error, "could not report Android TLS initialization failure to the host");
                }
                std::ptr::null_mut()
            }
        },
    }
}
