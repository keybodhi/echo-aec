import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'

const CABLE_INPUT_KEYWORD = 'CABLE Input'

export default function App() {
  const [devices, setDevices] = useState({ mics: [], loopbacks: [], outputs: [] })
  const [micDevice, setMicDevice] = useState('')
  const [loopbackDevice, setLoopbackDevice] = useState('')
  const [virtualMicDevice, setVirtualMicDevice] = useState('')
  const [isRunning, setIsRunning] = useState(false)
  const [notice, setNotice] = useState('')
  const [vbcableInstalled, setVbcableInstalled] = useState(true)
  const [installing, setInstalling] = useState(false)
  const [installMsg, setInstallMsg] = useState('')

  function findCableInput(outputs) {
    return outputs.find(d => d.name.includes(CABLE_INPUT_KEYWORD))
  }

  async function loadDevices() {
    const devs = await invoke('list_devices')
    setDevices(devs)
    setVbcableInstalled(!!findCableInput(devs.outputs))
    return devs
  }

  useEffect(() => {
    async function init() {
      try {
        const devs = await loadDevices()
        const cfg = await invoke('get_config')
        const missing = []

        if (cfg.mic_device) {
          if (devs.mics.some(d => d.id === cfg.mic_device)) {
            setMicDevice(cfg.mic_device)
          } else {
            missing.push(`麦克风 "${cfg.mic_device}"`)
          }
        }
        if (cfg.loopback_device) {
          if (devs.loopbacks.some(d => d.id === cfg.loopback_device)) {
            setLoopbackDevice(cfg.loopback_device)
          } else {
            missing.push(`监听设备 "${cfg.loopback_device}"`)
          }
        }
        if (cfg.virtual_mic_device) {
          if (devs.outputs.some(d => d.id === cfg.virtual_mic_device)) {
            setVirtualMicDevice(cfg.virtual_mic_device)
          } else {
            missing.push(`虚拟麦克风 "${cfg.virtual_mic_device}"`)
          }
        } else {
          const cable = findCableInput(devs.outputs)
          if (cable) {
            setVirtualMicDevice(cable.id)
          }
        }

        if (missing.length > 0) {
          setNotice(`上次的设备未找到: ${missing.join(', ')}`)
        }
      } catch (e) {
        console.error(e)
      }
    }
    init()
  }, [])

  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const data = await invoke('get_status')
        setIsRunning(data.is_running)
      } catch (e) {
        console.error(e)
      }
    }, 1000)
    return () => clearInterval(interval)
  }, [])

  const handleInstallVbcable = async () => {
    setInstalling(true)
    setInstallMsg('')
    try {
      const msg = await invoke('install_vbcable')
      setInstallMsg(msg)
      setTimeout(async () => {
        try {
          const devs = await invoke('refresh_devices')
          setDevices(devs)
          const cable = findCableInput(devs.outputs)
          setVbcableInstalled(!!cable)
          if (cable && !virtualMicDevice) {
            setVirtualMicDevice(cable.id)
          }
        } catch (e) {
          console.error(e)
        }
        setInstalling(false)
      }, 8000)
    } catch (e) {
      setInstallMsg('安装失败: ' + e)
      setInstalling(false)
    }
  }

  const handleStart = async () => {
    if (!micDevice || !loopbackDevice || !virtualMicDevice) {
      alert('请选择所有设备')
      return
    }
    try {
      await invoke('start_processing', {
        micDevice,
        loopbackDevice,
        virtualMicDevice,
      })
    } catch (e) {
      alert('启动失败: ' + e)
    }
  }

  const handleStop = async () => {
    try {
      await invoke('stop_processing')
    } catch (e) {
      console.error(e)
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-4">
      <div className="bg-gray-800 rounded-xl p-8 w-full max-w-lg space-y-6">
        <h1 className="text-2xl font-bold text-center">Echo AEC</h1>
        <p className="text-center text-sm text-gray-400">回声消除 - 虚拟音频设备</p>

        {notice && (
          <div className="bg-yellow-900/50 border border-yellow-600 rounded px-3 py-2 text-sm text-yellow-200">
            {notice}
          </div>
        )}

        {!vbcableInstalled && (
          <div className="bg-red-900/50 border border-red-600 rounded px-3 py-3 text-sm text-red-200 space-y-2">
            <p>未检测到 VB-CABLE 虚拟声卡，这是本软件正常工作的必需组件。</p>
            <button
              onClick={handleInstallVbcable}
              disabled={installing}
              className="w-full py-2 rounded bg-red-600 hover:bg-red-700 font-semibold disabled:opacity-50"
            >
              {installing ? '安装中（请在 UAC 弹窗中确认）...' : '一键安装 VB-CABLE（内置官方安装包）'}
            </button>
            {installMsg && <p className="text-yellow-200">{installMsg}</p>}
          </div>
        )}

        <div className="space-y-4">
          <div className="space-y-2">
            <label className="text-sm text-gray-400">麦克风 (输入设备)</label>
            <select
              className="w-full bg-gray-700 rounded px-3 py-2 text-sm"
              value={micDevice}
              onChange={e => setMicDevice(e.target.value)}
            >
              <option value="">选择麦克风...</option>
              {devices.mics.map(d => (
                <option key={d.id} value={d.id}>{d.name}</option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label className="text-sm text-gray-400">系统音频监听 (Loopback 设备)</label>
            <select
              className="w-full bg-gray-700 rounded px-3 py-2 text-sm"
              value={loopbackDevice}
              onChange={e => setLoopbackDevice(e.target.value)}
            >
              <option value="">选择监听设备...</option>
              {devices.loopbacks.map(d => (
                <option key={d.id} value={d.id}>{d.name}</option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label className="text-sm text-gray-400">虚拟麦克风 (CABLE Input)</label>
            <select
              className="w-full bg-gray-700 rounded px-3 py-2 text-sm"
              value={virtualMicDevice}
              onChange={e => setVirtualMicDevice(e.target.value)}
            >
              <option value="">选择虚拟麦克风...</option>
              {devices.outputs.map(d => (
                <option key={d.id} value={d.id}>{d.name}</option>
              ))}
            </select>
          </div>
        </div>

        <button
          onClick={isRunning ? handleStop : handleStart}
          disabled={!micDevice || !loopbackDevice || !virtualMicDevice}
          className={`w-full py-3 rounded-lg font-semibold transition disabled:opacity-50 disabled:cursor-not-allowed ${
            isRunning
              ? 'bg-red-600 hover:bg-red-700'
              : 'bg-green-600 hover:bg-green-700'
          }`}
        >
          {isRunning ? '停止处理' : '启动 AEC'}
        </button>

        <div className="text-center text-sm text-gray-500">
          状态: {isRunning ? '运行中' : '已停止'}
        </div>

        <div className="text-center text-xs text-gray-600 border-t border-gray-700 pt-3">
          虚拟声卡驱动: VB-CABLE (<a href="https://vb-audio.com/Cable/" target="_blank" rel="noreferrer" className="text-gray-500 underline hover:text-gray-400">www.vb-cable.com</a>)
          <br />VB-CABLE 是 Donationware，欢迎捐款支持
        </div>
      </div>
    </div>
  )
}
