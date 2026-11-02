# Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.

param(
    [Parameter(Mandatory = $true)]
    [string] $Path
)

$ErrorActionPreference = 'Stop'
$signature = Get-AuthenticodeSignature -LiteralPath $Path
if ($signature.Status -ne 'Valid') {
    Write-Error $signature.Status
    exit 2
}

$subject = $signature.SignerCertificate.Subject
# Compare the whole O= field, so a name that only contains "Microsoft Corporation" does not pass.
$organization = $null
if ($subject -match '(?:^|,\s*)O=(?:"(?<quoted>[^"]*)"|(?<plain>[^,]*))') {
    $organization = if ($null -ne $Matches['quoted']) { $Matches['quoted'] } else { "$($Matches['plain'])".Trim() }
}
if ($organization -cne 'Microsoft Corporation') {
    Write-Error $subject
    exit 3
}

exit 0
