[System.Threading.Thread]::CurrentThread.CurrentUICulture = 'en-US';
    # [System.Threading.Thread]::CurrentThread.CurrentCulture = 'en-US';
    $query = “Select * from Win32_Bios”;
    Get-WmiObject -Query $query;
    $query = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem"
    Get-WmiObject -Query $query;
    
    # Get-WmiObject -Class Win32_DiskPartition  ;

    # Get-WmiObject -Class Win32_DiskPartition  | Select-Object -Property *;

    $query = "SELECT Index,DiskIndex,Type,PrimaryPartition,Bootable,BootPartition,BlockSize,Size,StartingOffset  from Win32_DiskPartition"
    Get-WmiObject -Query $query;
    SELECT Caption,Bootable,Size,NumberOfBlocks,Type,BootPartition FROM Win32_DiskPartition where DiskIndex=0 and Index=5