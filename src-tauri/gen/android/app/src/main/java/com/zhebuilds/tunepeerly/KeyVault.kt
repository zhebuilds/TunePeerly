// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

package com.zhebuilds.tunepeerly

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Wraps the library database key with an AES-256-GCM key that lives in the Android Keystore and
 * cannot be read out of it. `db.key` holds only the wrapped bytes: version, IV, ciphertext and tag.
 * Uninstalling the app deletes the Keystore key, so a copied `db.key` opens nothing.
 */
object KeyVault {
  private const val ALIAS = "tunepeerly-db-key"
  private const val VERSION: Byte = 1
  private const val IV_LEN = 12
  private const val TAG_BITS = 128

  @Synchronized
  private fun key(): SecretKey {
    val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
    generator.init(
      KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setKeySize(256)
        .build()
    )
    return generator.generateKey()
  }

  fun seal(plain: ByteArray): ByteArray {
    val cipher = Cipher.getInstance("AES/GCM/NoPadding")
    cipher.init(Cipher.ENCRYPT_MODE, key())
    val iv = cipher.iv
    require(iv.size == IV_LEN) { "unexpected IV length" }
    return byteArrayOf(VERSION) + iv + cipher.doFinal(plain)
  }

  fun unseal(sealed: ByteArray): ByteArray {
    require(sealed.size > 1 + IV_LEN && sealed[0] == VERSION) { "db.key is not a wrapped key" }
    val cipher = Cipher.getInstance("AES/GCM/NoPadding")
    cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(TAG_BITS, sealed, 1, IV_LEN))
    return cipher.doFinal(sealed, 1 + IV_LEN, sealed.size - 1 - IV_LEN)
  }
}
