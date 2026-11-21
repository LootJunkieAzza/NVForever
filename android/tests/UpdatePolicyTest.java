package org.nvforever.android;

/** Pure-Java unit tests for UpdatePolicy (APK names, HTTPS/GitHub URLs, SHA-256, progress). */
public final class UpdatePolicyTest {
    private static int checks = 0;

    private static void check(boolean condition, String detail) {
        checks++;
        if (!condition) throw new AssertionError("FAILED: " + detail);
    }

    public static void main(String[] args) {
        check(UpdatePolicy.safeApkName("NVForever-v0.8.1-android-arm64.apk"), "valid apk name");
        check(!UpdatePolicy.safeApkName("../evil.apk"), "reject traversal");
        check(!UpdatePolicy.safeApkName("sub/file.apk"), "reject slash");
        check(!UpdatePolicy.safeApkName("archive.zip"), "reject non-apk");
        check(!UpdatePolicy.safeApkName(".apk"), "reject empty stem");

        String tag = "v0.8.1";
        String name = "NVForever-v0.8.1-android-arm64.apk";
        String official = UpdatePolicy.REPO_DOWNLOADS + tag + "/" + name;
        check(UpdatePolicy.isRepoReleaseAssetUrl(official, tag, name), "official release URL");
        check(!UpdatePolicy.isRepoReleaseAssetUrl("https://evil.example/" + name, tag, name), "reject third-party host");
        check(!UpdatePolicy.isRepoReleaseAssetUrl("http://github.com/LootJunkieAzza/NVForever/releases/download/v0.8.1/" + name, tag, name), "reject http");

        check(UpdatePolicy.isAllowedDownloadUrl(official), "allow github.com");
        check(UpdatePolicy.isAllowedDownloadUrl("https://objects.githubusercontent.com/github-production-release-asset/1234"), "allow objects.githubusercontent.com");
        check(UpdatePolicy.isAllowedDownloadUrl("https://release-assets.githubusercontent.com/asset/1234"), "allow release-assets.githubusercontent.com");
        check(UpdatePolicy.isAllowedDownloadUrl("https://github-releases.githubusercontent.com/asset/1234"), "allow github-releases.githubusercontent.com");
        check(!UpdatePolicy.isAllowedDownloadUrl("https://raw.githubusercontent.com/user/repo/main/app.apk"), "reject raw.githubusercontent.com");
        check(!UpdatePolicy.isAllowedDownloadUrl("https://gist.githubusercontent.com/user/123/raw/app.apk"), "reject gist.githubusercontent.com");
        check(!UpdatePolicy.isAllowedDownloadUrl("http://objects.githubusercontent.com/asset"), "reject http redirect");
        check(!UpdatePolicy.isAllowedDownloadUrl("https://untrusted.example.com/app.apk"), "reject arbitrary redirect host");

        byte[] digest = new byte[32];
        for (int i = 0; i < 32; i++) digest[i] = (byte) i;
        String hex = UpdatePolicy.hexSha256(digest);
        check(hex.length() == 64, "64 hex chars");
        check(UpdatePolicy.checksumMatches(hex.toUpperCase(), digest), "case-insensitive checksum match");
        check(!UpdatePolicy.checksumMatches("0".repeat(64), digest), "mismatch detected");
        check(!UpdatePolicy.checksumMatches("short", digest), "short hex rejected");

        check(UpdatePolicy.progressText(5L * 1024 * 1024, 20L * 1024 * 1024).contains("25%"), "progress percent");
        check(UpdatePolicy.progressText(1024 * 1024, -1).contains("1.0 MB"), "unknown total progress");

        System.out.println("UpdatePolicyTest: PASS (" + checks + " checks)");
    }
}
