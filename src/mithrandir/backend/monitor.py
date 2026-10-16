import os
import re
import glob
from typing import Dict, List, Optional
from .host_bridge import HostBridge

class SystemMonitor:
    """
    Monitors CPU, RAM, swap, disk, temperatures, and GPU metrics.
    Can read from /sys, /proc directly or through the host bridge.
    """

    _last_cpu_time = 0.0
    _last_cpu_total = 0
    _last_cpu_idle = 0

    @classmethod
    def get_memory_info(cls) -> Dict[str, float]:
        meminfo = HostBridge.read_host_file('/proc/meminfo')
        if not meminfo:
            return {'total_mb': 0, 'used_mb': 0, 'percent': 0, 'swap_total_mb': 0, 'swap_used_mb': 0, 'swap_percent': 0}

        data = {}
        for line in meminfo.splitlines():
            parts = line.split(':')
            if len(parts) == 2:
                key = parts[0].strip()
                val = parts[1].strip().split()[0]
                try:
                    data[key] = int(val)
                except ValueError:
                    pass

        total_kb = data.get('MemTotal', 1)
        avail_kb = data.get('MemAvailable', total_kb)
        used_kb = total_kb - avail_kb

        swap_total_kb = data.get('SwapTotal', 0)
        swap_free_kb = data.get('SwapFree', 0)
        swap_used_kb = swap_total_kb - swap_free_kb

        return {
            'total_mb': round(total_kb / 1024, 1),
            'used_mb': round(used_kb / 1024, 1),
            'percent': round((used_kb / total_kb) * 100, 1),
            'swap_total_mb': round(swap_total_kb / 1024, 1),
            'swap_used_mb': round(swap_used_kb / 1024, 1),
            'swap_percent': round((swap_used_kb / max(1, swap_total_kb)) * 100, 1)
        }

    @classmethod
    def get_cpu_percent(cls) -> float:
        stat = HostBridge.read_host_file('/proc/stat')
        if not stat:
            return 0.0

        cpu_line = stat.splitlines()[0]
        fields = [int(x) for x in cpu_line.split()[1:]]
        idle = fields[3] + (fields[4] if len(fields) > 4 else 0)
        total = sum(fields)

        diff_total = total - cls._last_cpu_total
        diff_idle = idle - cls._last_cpu_idle

        cls._last_cpu_total = total
        cls._last_cpu_idle = idle

        if diff_total == 0:
            return 0.0

        usage = (1.0 - (diff_idle / diff_total)) * 100.0
        return max(0.0, min(100.0, round(usage, 1)))

    @classmethod
    def get_disk_usage(cls, path: str = '/') -> Dict[str, float]:
        code, out, _ = HostBridge.run_command(['df', '-B1', path], timeout=5)
        if code == 0 and len(out.splitlines()) > 1:
            line = out.splitlines()[1]
            parts = line.split()
            if len(parts) >= 4:
                try:
                    total_bytes = int(parts[1])
                    used_bytes = int(parts[2])
                    avail_bytes = int(parts[3])
                    return {
                        'total_gb': round(total_bytes / (1024**3), 2),
                        'used_gb': round(used_bytes / (1024**3), 2),
                        'free_gb': round(avail_bytes / (1024**3), 2),
                        'percent': round((used_bytes / max(1, total_bytes)) * 100, 1)
                    }
                except ValueError:
                    pass
        return {'total_gb': 0, 'used_gb': 0, 'free_gb': 0, 'percent': 0}

    @classmethod
    def get_temperatures(cls) -> Dict[str, float]:
        """
        Reads system thermal zones and hwmon temperatures.
        Returns a dict with key 'cpu_temp' (and package temp if available).
        """
        temps = []
        # Check thermal zones
        for zone in range(8):
            path = f"/sys/class/thermal/thermal_zone{zone}/temp"
            val = HostBridge.read_host_file(path)
            if val:
                try:
                    t = float(val.strip()) / 1000.0
                    if 10.0 <= t <= 115.0:
                        temps.append(t)
                except ValueError:
                    pass

        # Check hwmon
        for hwmon in range(8):
            for t_idx in range(1, 6):
                path = f"/sys/class/hwmon/hwmon{hwmon}/temp{t_idx}_input"
                val = HostBridge.read_host_file(path)
                if val:
                    try:
                        t = float(val.strip()) / 1000.0
                        if 10.0 <= t <= 115.0:
                            temps.append(t)
                    except ValueError:
                        pass

        cpu_temp = round(max(temps), 1) if temps else 0.0
        return {'cpu_temp': cpu_temp}

    _nvidia_available: Optional[bool] = None

    @classmethod
    def get_gpu_info(cls) -> List[Dict[str, any]]:
        """
        Detects GPU0 and GPU1 (Intel, AMD, Nvidia) and returns utilization/model info.
        Avoids blocking on nvidia-smi if the NVIDIA kernel driver is not loaded.
        """
        gpus = []

        # Only invoke nvidia-smi if the NVIDIA kernel module is loaded
        if cls._nvidia_available is None:
            cls._nvidia_available = bool(
                os.path.exists('/proc/driver/nvidia/version')
                or HostBridge.file_exists('/proc/driver/nvidia/version')
            ) and (HostBridge.which('nvidia-smi') is not None)

        if cls._nvidia_available:
            code, out, _ = HostBridge.run_command(
                ['nvidia-smi', '--query-gpu=name,utilization.gpu,temperature.gpu,memory.used,memory.total', '--format=csv,noheader,nounits'],
                timeout=2
            )
            if code == 0 and out.strip():
                lines = out.strip().splitlines()
                for idx, line in enumerate(lines):
                    parts = [p.strip() for p in line.split(',')]
                    if len(parts) >= 3:
                        name = parts[0]
                        try:
                            util = float(parts[1])
                        except (ValueError, IndexError):
                            util = 0.0
                        try:
                            temp = float(parts[2])
                        except (ValueError, IndexError):
                            temp = 0.0
                        gpus.append({
                            'id': f'GPU{idx}',
                            'name': name,
                            'util_percent': util,
                            'temp': temp
                        })
            elif code == -1:
                # Timed out; disable repeated blocking nvidia-smi queries
                cls._nvidia_available = False

        # Check DRM cards (/sys/class/drm/card0, card1)
        for card_idx in [0, 1]:
            # Avoid duplicating if already matched by nvidia
            if len(gpus) > card_idx:
                continue

            card_path = f"/sys/class/drm/card{card_idx}"
            # Check if device exists
            vendor_path = f"{card_path}/device/vendor"
            vendor_str = HostBridge.read_host_file(vendor_path)
            if not vendor_str:
                continue

            vendor_id = vendor_str.strip().lower()
            vendor_name = "GPU"
            if "0x8086" in vendor_id:
                vendor_name = "Intel Graphics"
            elif "0x1002" in vendor_id:
                vendor_name = "AMD Radeon"
            elif "0x10de" in vendor_id:
                vendor_name = "NVIDIA"

            # Check busy percent
            busy_path = f"{card_path}/device/gpu_busy_percent"
            busy_str = HostBridge.read_host_file(busy_path)
            busy = 0.0
            if busy_str:
                try:
                    busy = float(busy_str.strip())
                except ValueError:
                    busy = 0.0

            gpus.append({
                'id': f'GPU{card_idx}',
                'name': vendor_name,
                'util_percent': busy,
                'temp': 0.0
            })

        return gpus

    _last_net_time: float = 0.0
    _last_net_rx: int = 0
    _last_net_tx: int = 0

    @classmethod
    def get_network_rates(cls) -> Dict[str, any]:
        """
        Reads /proc/net/dev to calculate instantaneous download/upload transfer rates.
        Returns download_str (e.g. '1.2 MB/s') and upload_str (e.g. '240 KB/s').
        """
        import time
        net_dev = HostBridge.read_host_file('/proc/net/dev')
        if not net_dev:
            return {'rx_bytes_sec': 0, 'tx_bytes_sec': 0, 'rx_str': '0 KB/s', 'tx_str': '0 KB/s'}

        rx_total = 0
        tx_total = 0
        lines = net_dev.splitlines()
        for line in lines[2:]:
            parts = line.split(':')
            if len(parts) == 2:
                iface = parts[0].strip()
                if iface in ('lo', 'docker0', 'virbr0') or iface.startswith('veth'):
                    continue
                vals = parts[1].split()
                if len(vals) >= 9:
                    try:
                        rx_total += int(vals[0])
                        tx_total += int(vals[8])
                    except ValueError:
                        pass

        now = time.time()
        time_diff = now - cls._last_net_time if cls._last_net_time > 0 else 0.0
        rx_diff = rx_total - cls._last_net_rx if cls._last_net_rx > 0 else 0
        tx_diff = tx_total - cls._last_net_tx if cls._last_net_tx > 0 else 0

        cls._last_net_time = now
        cls._last_net_rx = rx_total
        cls._last_net_tx = tx_total

        if time_diff <= 0 or rx_diff < 0 or tx_diff < 0:
            rx_rate = 0.0
            tx_rate = 0.0
        else:
            rx_rate = rx_diff / time_diff
            tx_rate = tx_diff / time_diff

        def fmt_rate(rate_bytes: float) -> str:
            if rate_bytes >= 1024 * 1024:
                return f"{rate_bytes / (1024 * 1024):.1f} MB/s"
            elif rate_bytes >= 1024:
                return f"{rate_bytes / 1024:.1f} KB/s"
            return f"{int(rate_bytes)} B/s"

        return {
            'rx_bytes_sec': rx_rate,
            'tx_bytes_sec': tx_rate,
            'rx_str': fmt_rate(rx_rate),
            'tx_str': fmt_rate(tx_rate)
        }

    @classmethod
    def get_battery_info(cls) -> Dict[str, any]:
        """
        Retrieves battery health and capacity telemetry from /sys/class/power_supply or upower.
        Returns a dictionary containing:
        - present (bool): True if at least one battery is found and present
        - real_capacity_wh (float): Actual maximum capacity when full in Wh
        - design_capacity_wh (float): Original design maximum capacity in Wh
        - capacity_percent (float): Battery health ratio (real / design * 100)
        - energy_now_wh (float): Current stored energy in Wh
        - status (str): Charging, Discharging, Full, Not charging, etc.
        - model (str): Battery model name
        - vendor (str): Battery manufacturer
        """
        power_dir = '/sys/class/power_supply'
        devices = []
        if os.path.isdir(power_dir):
            try:
                devices = os.listdir(power_dir)
            except Exception:
                pass

        if not devices and HostBridge.is_flatpak():
            code, out, _ = HostBridge.run_command(['ls', power_dir], timeout=3)
            if code == 0 and out:
                devices = out.split()

        total_real_wh = 0.0
        total_design_wh = 0.0
        total_now_wh = 0.0
        found_any = False
        status = 'Unknown'
        model = ''
        vendor = ''

        for dev in sorted(devices):
            dev_path = f'{power_dir}/{dev}'
            type_str = HostBridge.read_host_file(f'{dev_path}/type')
            is_battery = False
            if type_str and 'battery' in type_str.strip().lower():
                is_battery = True
            elif not type_str and dev.lower().startswith('bat'):
                is_battery = True

            if not is_battery:
                continue

            present_str = HostBridge.read_host_file(f'{dev_path}/present')
            if present_str and present_str.strip() == '0':
                continue

            real_wh = 0.0
            design_wh = 0.0
            now_wh = 0.0

            # 1. Try direct energy readings (in micro-Watt-hours: µWh)
            ef_str = HostBridge.read_host_file(f'{dev_path}/energy_full')
            efd_str = HostBridge.read_host_file(f'{dev_path}/energy_full_design')
            en_str = HostBridge.read_host_file(f'{dev_path}/energy_now')

            if ef_str and efd_str:
                try:
                    real_wh = float(ef_str.strip()) / 1_000_000.0
                    design_wh = float(efd_str.strip()) / 1_000_000.0
                    if en_str:
                        now_wh = float(en_str.strip()) / 1_000_000.0
                except ValueError:
                    pass
            else:
                # 2. Fallback to charge readings (in µAh) and voltage (in µV)
                cf_str = HostBridge.read_host_file(f'{dev_path}/charge_full')
                cfd_str = HostBridge.read_host_file(f'{dev_path}/charge_full_design')
                cn_str = HostBridge.read_host_file(f'{dev_path}/charge_now')
                vmin_str = HostBridge.read_host_file(f'{dev_path}/voltage_min_design')
                vnow_str = HostBridge.read_host_file(f'{dev_path}/voltage_now')
                v_uV = None
                if vmin_str:
                    try:
                        v_uV = float(vmin_str.strip())
                    except ValueError:
                        pass
                elif vnow_str:
                    try:
                        v_uV = float(vnow_str.strip())
                    except ValueError:
                        pass

                if cf_str and cfd_str and v_uV and v_uV > 0:
                    try:
                        real_wh = (float(cf_str.strip()) * v_uV) / 1e12
                        design_wh = (float(cfd_str.strip()) * v_uV) / 1e12
                        if cn_str:
                            now_wh = (float(cn_str.strip()) * v_uV) / 1e12
                    except ValueError:
                        pass

            if design_wh > 0:
                found_any = True
                total_real_wh += real_wh
                total_design_wh += design_wh
                total_now_wh += now_wh
                st = HostBridge.read_host_file(f'{dev_path}/status')
                if st and st.strip():
                    status = st.strip()
                md = HostBridge.read_host_file(f'{dev_path}/model_name')
                if md and md.strip():
                    model = md.strip()
                vn = HostBridge.read_host_file(f'{dev_path}/manufacturer')
                if vn and vn.strip():
                    vendor = vn.strip()

        # 3. UPower fallback if sysfs produced no battery data
        if not found_any and HostBridge.which('upower'):
            code, out, _ = HostBridge.run_command(['upower', '-e'], timeout=3)
            if code == 0 and out:
                devs = [l.strip() for l in out.splitlines() if 'battery' in l.lower()]
                for u_dev in devs:
                    c, info, _ = HostBridge.run_command(['upower', '-i', u_dev], timeout=3)
                    if c != 0 or not info:
                        continue
                    data = {}
                    for line in info.splitlines():
                        parts = line.split(':', 1)
                        if len(parts) == 2:
                            data[parts[0].strip()] = parts[1].strip()

                    ef_m = re.search(r'([0-9.]+)\s*Wh', data.get('energy-full', ''))
                    efd_m = re.search(r'([0-9.]+)\s*Wh', data.get('energy-full-design', ''))
                    en_m = re.search(r'([0-9.]+)\s*Wh', data.get('energy', ''))
                    if ef_m and efd_m:
                        try:
                            r_wh = float(ef_m.group(1))
                            d_wh = float(efd_m.group(1))
                            if d_wh > 0:
                                total_real_wh += r_wh
                                total_design_wh += d_wh
                                if en_m:
                                    total_now_wh += float(en_m.group(1))
                                found_any = True
                                if 'state' in data:
                                    status = data['state'].capitalize()
                                if 'model' in data:
                                    model = data['model']
                                if 'vendor' in data:
                                    vendor = data['vendor']
                        except ValueError:
                            pass

        if not found_any or total_design_wh <= 0:
            return {
                'present': False,
                'real_capacity_wh': 0.0,
                'design_capacity_wh': 0.0,
                'energy_now_wh': 0.0,
                'capacity_percent': 0.0,
                'status': 'N/A',
                'model': '',
                'vendor': ''
            }

        cap_pct = round((total_real_wh / total_design_wh) * 100.0, 1)
        return {
            'present': True,
            'real_capacity_wh': round(total_real_wh, 2),
            'design_capacity_wh': round(total_design_wh, 2),
            'energy_now_wh': round(total_now_wh, 2),
            'capacity_percent': cap_pct,
            'status': status,
            'model': model,
            'vendor': vendor
        }

