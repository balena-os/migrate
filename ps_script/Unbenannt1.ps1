# $query = "SELECT *  from Win32_DiskPartition"
# $query = "SELECT Caption, DiskIndex, Index,Bootable,Size,NumberOfBlocks,Type,BootPartition,StartingOffset FROM Win32_DiskPartition"
# Get-WmiObject -Query $query;
[System.Threading.Thread]::CurrentThread.CurrentCulture = 'en-US'; Get-WmiObject -Class Win32_DiskPartition  | Select-Object -Property *;
    