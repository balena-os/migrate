    [System.Threading.Thread]::CurrentThread.CurrentUICulture = 'en-US';
    # [System.Threading.Thread]::CurrentThread.CurrentCulture = 'en-US';
    $query = “Select * from Win32_Bios”;
    Get-WmiObject -Query $query;
    $query = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem"
    Get-WmiObject -Query $query;
    
    Get-WmiObject -Class Win32_DiskPartition  | Select-Object -Property *;


  

