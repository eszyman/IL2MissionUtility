# Read one image with the Windows OCR engine and print the text.
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
    $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
})[0]

function Get-Inner($ErrorRecord) {
    $ex = $ErrorRecord.Exception
    if (-not $ex) { return "$ErrorRecord" }
    while ($ex.InnerException) { $ex = $ex.InnerException }
    return "$($ex.GetType().Name): $($ex.Message)"
}

function Await($WinRtTask, $ResultType) {
    $asTask = $asTaskGeneric.MakeGenericMethod($ResultType)
    $netTask = $asTask.Invoke($null, @($WinRtTask))
    $null = $netTask.Wait()
    if ($netTask.IsFaulted) { throw $netTask.Exception.GetBaseException() }
    return $netTask.Result
}

function Read-OcrText([string]$ImagePath) {
    $null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics.Imaging, ContentType = WindowsRuntime]
    $null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
    $bytes = $null
    foreach ($try in 1..5) {
        try {
            $bytes = [System.IO.File]::ReadAllBytes($ImagePath)
            break
        } catch {
            if ($try -eq 5) { throw }
            Start-Sleep -Milliseconds (150 * $try)
        }
    }
    # The capture has just written this file. Windows OCR's file opener faults
    # while that write is still settling, so the bytes are read into a new file.
    $temp = Join-Path $env:TEMP ("il2-ocr-" + [guid]::NewGuid().ToString("n") + ".png")
    [System.IO.File]::WriteAllBytes($temp, $bytes)
    try {
        $file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($temp)) ([Windows.Storage.StorageFile])
        $stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
        $decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
        $bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
        $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
        if (-not $engine) { throw 'Windows OCR has no language installed' }
        $result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
        return $result.Text
    } finally {
        Remove-Item -LiteralPath $temp -ErrorAction SilentlyContinue
    }
}

$last = 'Windows OCR failed'
foreach ($try in 1..4) {
    try {
        Write-Output (Read-OcrText $Path)
        exit 0
    } catch {
        $last = Get-Inner $_
        Start-Sleep -Milliseconds (250 * $try)
    }
}
throw $last
