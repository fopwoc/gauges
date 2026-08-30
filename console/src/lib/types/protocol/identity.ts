export interface ProbeIdentity {
  hostname: string;
  distro: string;
  distroVersion: string;
  kernelVersion: string;
  ipAddress: string | null;
  cpuModel: string | null;
  gpuTypes: string[];
}
