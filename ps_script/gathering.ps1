  # [System.Threading.Thread]::CurrentThread.CurrentUICulture = 'en-US';
    
    # $query = “Select * from Win32_Bios”;
    #Get-WmiObject -Query $query;
    #$query = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem"
    #Get-WmiObject -Query $query;
    
    # Get-WmiObject -Class Win32_DiskPartition  ;

    # Get-WmiObject -Class Win32_DiskPartition  | Select-Object -Property *;

    #$query = "SELECT Index,DiskIndex,Type,PrimaryPartition,Bootable,BootPartition,BlockSize,Size,StartingOffset  from Win32_DiskPartition  WHERE DiskIndex=0 and Index=1"
    # $query="SELECT * from Win32_DiskDrive" 
    # [System.Threading.Thread]::CurrentThread.CurrentCulture = 'en-US';Get-WmiObject -Query $query;
    # Get-WmiObject -Class Win32_DiskDrive  | Select-Object -Property *;

    $query="SELECT Caption, Name, Size, MediaType, Status, BytesPerSector, Partitions, CompressionMethod FROM Win32_DiskDrive"
    [System.Threading.Thread]::CurrentThread.CurrentCulture = 'en-US';Get-WmiObject -Query $query;

    