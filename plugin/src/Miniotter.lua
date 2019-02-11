local RunService = game:GetService("RunService")

local Rojo = script:FindFirstAncestor("Rojo")

local Roact = require(Rojo.Roact)

local Miniotter = {}
Miniotter.__index = Miniotter

function Miniotter.new(initialValue)
	local binding, setBindingValue = Roact.createBinding(initialValue)

	local self = {
		binding = binding,
		setBindingValue = setBindingValue,
		state = {
			value = initialValue,
			complete = false,
		},

		goal = nil,
		connection = nil,
		completeCallback = nil,
	}

	return setmetatable(self, Miniotter)
end

function Miniotter:start()
	self.connection = RunService.RenderStepped:Connect(function(dt)
		self:step(dt)
	end)
end

function Miniotter:stop()
	if self.connection ~= nil then
		self.connection:Disconnect()
		self.connection = nil
	end
end

function Miniotter:setGoal(goal, completeCallback)
	if self.completeCallback ~= nil then
		self.completeCallback(false)
		self.completeCallback = nil
	end

	self.goal = goal
	self:start()
	self.completeCallback = completeCallback
end

function Miniotter:step(dt)
	if self.goal ~= nil then
		self.state = self.goal:step(self.state, dt)
		self.setBindingValue(self.state.value)

		if self.state.complete then
			self:stop()

			if self.completeCallback ~= nil then
				self.completeCallback(true)
				self.completeCallback = nil
			end
		end
	end
end

return Miniotter