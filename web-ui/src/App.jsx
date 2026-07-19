import { useState, useEffect } from 'react'

export default function App() {
  const [devices, setDevices] = useState({ mics: [], loopbacks: [], outputs: [] })
  const [micDevice, setMicDevice] = useState('')
  const [loopbackDevice, setLoopbackDevice] = useState('')
  const [virtualMicDevice, setVirtualMicDevice] = useState('')
  const [isRunning, setIsRunning] = useState(false)

  useEffect(() => {
    fetch('/api/devices')
      .then(r => r.json())
      .then(setDevices)
      .catch(console.error)
  }, [])

  useEffect(() => {
    const interval = setInterval(() => {
      fetch('/api/status')
        .then(r => r.json())
        .then(data => setIsRunning(data.is_running))
        .catch(console.error)
    }, 1000)
    return () => clearInterval(interval)
  }, [])

  const handleStart = async () => {
    if (!micDevice || !loopbackDevice || !virtualMicDevice) {
      alert('请选择所有设备')
      return
    }
    const res = await fetch('/api/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        mic_device: micDevice,
        loopback_device: loopbackDevice,
        virtual_mic_device: virtualMicDevice,
      }),
    })
    if (!res.ok) {
      alert('启动失败: ' + await res.text())
    }
  }

  const handleStop = async () => {
    await fetch('/api/stop', { method: 'POST' })
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-4">
      <div className="bg-gray-800 rounded-xl p-8 w-full max-w-lg space-y-6">
        <h1 className="text-2xl font-bold text-center">Echo AEC</h1>
        <p className="text-center text-sm text-gray-400">回声消除 - 虚拟音频设备</p>

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
            <label className="text-sm text-gray-400">虚拟麦克风 (Scream 输出)</label>
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
