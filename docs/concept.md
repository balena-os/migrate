# Migrate Concept

Migration takes place in two stages that are described in detail in the following chapters.

## Stage 1 

I stage 1 a platform dependent executable is executed with admin privileges. 
Command line options determine the primary mode of operation which can be either

- Standalone - The stage 1 executable will rely on local resources and not try to establish a connection to the balena cloud backend. All resources including the Balena OS image and the migration envoronment have to be supplied locally.

- Agent Mode - The stage 1 executablr acts as an agent. It tries to connect to the Balena cloud backend. The Balena OS image can be dynamically configured and downloaded from the backend. As long term goal the agents integrates with the dashboard and allows the configuration and initiation of the migration  from the dashboard. Functionality currently implemented in migdb (fleet migration) can be partially or completely implemented in the dashboard.

### Configuraton 

The stage 1 executable can be configured using command line parameters and / or a configuration file. 
Configuration file can be in yaml syntax.


### Standalone Migration

Required resources are:

- A migration environment containing:
    - Migration kernel & initramfs
    - for UEFI environments: uefi loader
    - for Legacy systems 
- A balenaOS image
- config.json file 

Optional resources are 
- WIFI credentials
- Additional network manager files
- Several options governing the migration process.


    - 




