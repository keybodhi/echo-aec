import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'

export default function App() {
  const [devices, setDevices] = useState({ mics: [], loopbacks: [], outputs: [] })
  const [micDevice, setMicDevice] = useState('')
  const [loopbackDevice, setLoopbackDevice] = useState('')
  const [virtualMicDevice, setVirtualMicDevice] = useState('')
  const [isRunning, setIsRunning] = useState(false)
  const [notice, setNotice] = useState('')

  useEffect(() => {
    async function init() {
      try {
        const devs = await invoke('list_devices')
        setDevices(devs)

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
      </div>
    </div>
  )
}
